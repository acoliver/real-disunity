//! A stage's module, parsed and validated, with the bindings and constants it declares.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};

use crate::binding::ResourceLayout;
use crate::constants::BoundConstantLayout;
pub use crate::reflect::{Reflected, StorageImageUse};
use crate::reflect::{SamplingPair, reflect, sampling_pairs};

/// A pipeline stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageKind {
    Vertex,
    Fragment,
    Compute,
}

impl StageKind {
    pub(crate) fn naga(self) -> naga::ShaderStage {
        match self {
            Self::Vertex => naga::ShaderStage::Vertex,
            Self::Fragment => naga::ShaderStage::Fragment,
            Self::Compute => naga::ShaderStage::Compute,
        }
    }

    pub(crate) fn visibility(self) -> wgpu::ShaderStages {
        match self {
            Self::Vertex => wgpu::ShaderStages::VERTEX,
            Self::Fragment => wgpu::ShaderStages::FRAGMENT,
            Self::Compute => wgpu::ShaderStages::COMPUTE,
        }
    }
}

/// Where a stage's code comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageSource {
    /// SPIR-V words that naga reads. The vertex position output is flipped in Y, which turns
    /// the SPIR-V clip space (Y down) into the one wgpu draws with.
    SpirV(Arc<[u32]>),
    /// WGSL text, parsed with wgpu's own clip convention: the position output is used as
    /// written.
    Wgsl(Arc<str>),
}

impl StageSource {
    fn tag(&self) -> &'static str {
        match self {
            Self::SpirV(_) => "spirv",
            Self::Wgsl(_) => "wgsl",
        }
    }

    fn update(&self, hasher: &mut blake3::Hasher) {
        match self {
            Self::SpirV(words) => {
                for word in words.iter() {
                    hasher.update(&word.to_le_bytes());
                }
            }
            Self::Wgsl(text) => {
                hasher.update(text.as_bytes());
            }
        }
    }
}

/// One entry point of a module, for one stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageModule {
    pub source: StageSource,
    pub entry_point: String,
    pub kind: StageKind,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModuleError {
    #[error("naga cannot parse the module: {0}")]
    Parse(String),
    #[error("naga rejects the module: {0}")]
    Validate(String),
    #[error("naga panicked on the module: {0}")]
    Panicked(String),
    #[error("the module has no {kind:?} entry point named {name}")]
    EntryPointMissing { name: String, kind: StageKind },
    #[error("resource {name} is a {what}, which this renderer does not bind")]
    UnsupportedResource { name: String, what: String },
}

/// A pipeline-overridable constant a stage cannot be given.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OverrideError {
    #[error("the module declares no override named {name}")]
    Unknown { name: String },
    #[error("override {name} was given more than once")]
    Duplicate { name: String },
    #[error("override {name} is {expected}, and {value} is not")]
    Value {
        name: String,
        expected: &'static str,
        value: String,
    },
}

/// The resources and constant buffers a stage declares. A declaration is checked against the
/// resources the entry point uses when a program's layout is built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageDeclarations {
    pub resources: ResourceLayout,
    pub constants: Vec<BoundConstantLayout>,
}

/// What naga made of the source: the module and what validation learned of it. Every stage made
/// from one source shares one.
pub(crate) struct Parsed {
    pub(crate) module: naga::Module,
    info: ModuleInfo,
}

/// A stage ready to build pipelines from: parsed, validated, with its declarations.
///
/// Cloning shares the parsed module.
#[derive(Clone)]
pub struct StageProgram {
    module: StageModule,
    declarations: StageDeclarations,
    parsed: Arc<Parsed>,
    /// The resources the entry point uses.
    resources: Arc<[Reflected]>,
    /// The values of pipeline-overridable constants, ordered by the key wgpu is given.
    overrides: Arc<[(String, f64)]>,
    /// Identifies the compiled module: the source alone, so every entry point and every set of
    /// overrides of one source shares one.
    module_key: blake3::Hash,
    /// Identifies the pipeline stage: the source, entry point, stage and overrides.
    key: blake3::Hash,
}

impl std::fmt::Debug for StageProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageProgram")
            .field("entry_point", &self.module.entry_point)
            .field("kind", &self.module.kind)
            .field("key", &self.key)
            .field("overrides", &self.overrides)
            .finish_non_exhaustive()
    }
}

impl StageProgram {
    /// Parses and validates `module` with naga, reading the resources its entry point uses.
    /// Use [`sibling`](Self::sibling) for another entry point of the same source, which does not
    /// parse again.
    ///
    /// # Errors
    /// Fails with naga's parse or validation error, a missing entry point, or a resource that
    /// cannot be bound.
    pub fn new(module: StageModule, declarations: StageDeclarations) -> Result<Self, ModuleError> {
        let (parsed, resources) = guarded(|| {
            let parsed = parse(&module.source)?;
            let resources = reflected(&parsed, &module.entry_point, module.kind)?;
            Ok((parsed, resources))
        })?;
        let module_key = source_key(&module.source);
        let key = stage_key(module_key, &module, &[]);
        Ok(Self {
            module,
            declarations,
            parsed: Arc::new(parsed),
            resources,
            overrides: Arc::from([]),
            module_key,
            key,
        })
    }

    /// Another entry point of the same source, which shares this stage's parse and validation:
    /// a vertex and a fragment stage of one WGSL text cost one parse, and they compile as one
    /// shader module. The sibling has no declarations and no overrides; give it its own with
    /// [`with_declarations`](Self::with_declarations) and
    /// [`with_overrides`](Self::with_overrides).
    ///
    /// # Errors
    /// Fails with `EntryPointMissing` when the source has no `kind` entry point named
    /// `entry_point`, and with a resource that cannot be bound.
    pub fn sibling(&self, entry_point: &str, kind: StageKind) -> Result<Self, ModuleError> {
        let module = StageModule {
            source: self.module.source.clone(),
            entry_point: entry_point.to_owned(),
            kind,
        };
        let resources = guarded(|| reflected(&self.parsed, entry_point, kind))?;
        let key = stage_key(self.module_key, &module, &[]);
        Ok(Self {
            module,
            declarations: StageDeclarations::default(),
            parsed: self.parsed.clone(),
            resources,
            overrides: Arc::from([]),
            module_key: self.module_key,
            key,
        })
    }

    /// The same stage with pipeline-overridable constants set to `overrides`, which replace any
    /// the stage had. A constant is named as wgpu names it: by identifier, or by its `@id`
    /// written as a decimal number when it has one. A value is a number of the constant's type:
    /// 0 or 1 for `bool`, an integer in range for `u32` and `i32`, a finite number in range for
    /// the float types.
    ///
    /// The parsed module is shared, and so is the compiled module: overrides apply when a
    /// pipeline is created, and they are part of the pipeline's content key, so equal stages
    /// with different overrides are different pipelines over one shader module. The order they
    /// are given in does not matter. A constant without a default that the entry point uses and
    /// that is not set here is the GPU's to refuse when a pipeline is created.
    ///
    /// # Errors
    /// Fails, naming the constant, when the module declares no such override, when one is given
    /// twice and when a value is not one its type holds.
    pub fn with_overrides(&self, overrides: &[(&str, f64)]) -> Result<Self, OverrideError> {
        let overrides = checked_overrides(&self.parsed.module, overrides)?;
        let key = stage_key(self.module_key, &self.module, &overrides);
        Ok(Self {
            overrides: overrides.into(),
            key,
            ..self.clone()
        })
    }

    /// Whether both stages were made from one parse and validation of one module.
    #[must_use]
    pub fn shares_parse_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.parsed, &other.parsed)
    }

    /// The overrides set on the stage, ordered by name, as wgpu is given them.
    #[must_use]
    pub fn overrides(&self) -> &[(String, f64)] {
        &self.overrides
    }

    /// The same parsed stage with `declarations` instead.
    #[must_use]
    pub fn with_declarations(&self, declarations: StageDeclarations) -> Self {
        Self {
            declarations,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn module(&self) -> &StageModule {
        &self.module
    }

    #[must_use]
    pub fn declarations(&self) -> &StageDeclarations {
        &self.declarations
    }

    /// A hash of the source, entry point, stage and overrides, which identifies the stage of a
    /// pipeline.
    #[must_use]
    pub fn content_key(&self) -> blake3::Hash {
        self.key
    }

    /// A hash of the source, which identifies the compiled module that every entry point and
    /// every set of overrides of the source shares.
    #[must_use]
    pub(crate) fn module_key(&self) -> blake3::Hash {
        self.module_key
    }

    /// The overrides in the form pipeline creation takes them.
    pub(crate) fn constants(&self) -> Vec<(&str, f64)> {
        self.overrides
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect()
    }

    /// The resources the entry point uses, by set and binding.
    #[must_use]
    pub fn reflected(&self) -> &[Reflected] {
        &self.resources
    }

    /// The textures and samplers the entry point samples together, by address.
    pub(crate) fn sampling_pairs(&self) -> Vec<SamplingPair> {
        entry_index(&self.parsed, &self.module.entry_point, self.module.kind)
            .map(|entry| sampling_pairs(&self.parsed.module, &self.parsed.info, entry))
            .unwrap_or_default()
    }

    /// The module as naga parsed it.
    #[must_use]
    pub fn naga_module(&self) -> &naga::Module {
        &self.parsed.module
    }
}

/// The key wgpu takes a constant by: the `@id` as a decimal number when it has one, else its
/// identifier.
fn override_key(item: &naga::Override) -> Option<String> {
    item.id
        .map(|id| id.to_string())
        .or_else(|| item.name.clone())
}

/// `requested`, checked against the overrides `module` declares and ordered by key.
fn checked_overrides(
    module: &naga::Module,
    requested: &[(&str, f64)],
) -> Result<Vec<(String, f64)>, OverrideError> {
    let mut checked: Vec<(String, f64)> = Vec::with_capacity(requested.len());
    for &(name, value) in requested {
        let item = module
            .overrides
            .iter()
            .map(|(_, item)| item)
            .find(|item| override_key(item).as_deref() == Some(name))
            .ok_or_else(|| OverrideError::Unknown {
                name: name.to_owned(),
            })?;
        if checked.iter().any(|(known, _)| known == name) {
            return Err(OverrideError::Duplicate {
                name: name.to_owned(),
            });
        }
        let scalar = match module.types[item.ty].inner {
            naga::TypeInner::Scalar(scalar) => Some(scalar),
            _ => None,
        };
        if let Some(expected) = scalar.and_then(|scalar| refused_value(scalar, value)) {
            return Err(OverrideError::Value {
                name: name.to_owned(),
                expected,
                value: value.to_string(),
            });
        }
        checked.push((name.to_owned(), value));
    }
    checked.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(checked)
}

/// What a scalar type holds, when `value` is not one of its values.
fn refused_value(scalar: naga::Scalar, value: f64) -> Option<&'static str> {
    let integer = value.fract() == 0.0;
    let (expected, holds) = match (scalar.kind, scalar.width) {
        (naga::ScalarKind::Bool, _) => ("a bool, 0 or 1", [0.0, 1.0].contains(&value)),
        (naga::ScalarKind::Uint, 4) => (
            "a u32, an integer from 0 to 4294967295",
            integer && (0.0..=f64::from(u32::MAX)).contains(&value),
        ),
        (naga::ScalarKind::Sint, 4) => (
            "an i32, an integer from -2147483648 to 2147483647",
            integer && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value),
        ),
        (naga::ScalarKind::Float, 4) => (
            "an f32, a finite number within its range",
            value.is_finite() && value.abs() <= f64::from(f32::MAX),
        ),
        (naga::ScalarKind::Float, 2) => (
            "an f16, a finite number within its range",
            value.is_finite() && value.abs() <= 65504.0,
        ),
        _ => ("a number the type holds", value.is_finite()),
    };
    (!holds).then_some(expected)
}

const OP_ENTRY_POINT: u32 = 15;
const FIRST_INSTRUCTION: usize = 5;

/// The names of the entry points of a SPIR-V module, in the order it declares them. Words that
/// do not hold well-formed instructions end the list.
#[must_use]
pub fn entry_point_names(words: &[u32]) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = FIRST_INSTRUCTION;
    while let Some(&head) = words.get(at) {
        let Some(instruction) = usize::try_from(head >> 16)
            .ok()
            .filter(|&length| length > 0)
            .and_then(|length| words.get(at..at + length))
        else {
            break;
        };
        if head & 0xffff == OP_ENTRY_POINT {
            let bytes: Vec<u8> = instruction
                .get(3..)
                .unwrap_or_default()
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take_while(|&byte| byte != 0)
                .collect();
            names.push(String::from_utf8_lossy(&bytes).into_owned());
        }
        at += instruction.len();
    }
    names
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

fn guarded<T>(work: impl FnOnce() -> Result<T, ModuleError>) -> Result<T, ModuleError> {
    catch_unwind(AssertUnwindSafe(work))
        .map_err(|payload| ModuleError::Panicked(panic_text(payload.as_ref())))?
}

fn parse(source: &StageSource) -> Result<Parsed, ModuleError> {
    let module = match source {
        StageSource::SpirV(words) => parse_spirv(words)?,
        StageSource::Wgsl(text) => naga::front::wgsl::parse_str(text)
            .map_err(|error| ModuleError::Parse(error.emit_to_string(text)))?,
    };
    let info: ModuleInfo = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| ModuleError::Validate(format!("{:?}", error.into_inner())))?;
    Ok(Parsed { module, info })
}

/// The resources the `kind` entry point `entry_point` of `parsed` uses.
fn reflected(
    parsed: &Parsed,
    entry_point: &str,
    kind: StageKind,
) -> Result<Arc<[Reflected]>, ModuleError> {
    let entry = entry_index(parsed, entry_point, kind)?;
    Ok(reflect(&parsed.module, &parsed.info, entry)?.into())
}

/// The index of the `kind` entry point `entry_point` in `parsed`.
fn entry_index(parsed: &Parsed, entry_point: &str, kind: StageKind) -> Result<usize, ModuleError> {
    parsed
        .module
        .entry_points
        .iter()
        .position(|e| e.name == entry_point && e.stage == kind.naga())
        .ok_or_else(|| ModuleError::EntryPointMissing {
            name: entry_point.to_owned(),
            kind,
        })
}

/// A hash of the source alone.
fn source_key(source: &StageSource) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(source.tag().as_bytes());
    source.update(&mut hasher);
    hasher.finalize()
}

/// A hash of the source, entry point, stage and overrides, which are everything a pipeline's
/// stage is built from.
fn stage_key(
    source: blake3::Hash,
    module: &StageModule,
    overrides: &[(String, f64)],
) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(source.as_bytes());
    hasher.update(module.entry_point.as_bytes());
    hasher.update(format!("{:?}", module.kind).as_bytes());
    for (name, value) in overrides {
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update(&value.to_bits().to_le_bytes());
    }
    hasher.finalize()
}

fn parse_spirv(words: &[u32]) -> Result<naga::Module, ModuleError> {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    // The default options flip the vertex position's Y, which turns the SPIR-V clip space
    // (Y down) into the one wgpu draws with; the crate's convention tests pin the result.
    let options = naga::front::spv::Options::default();
    naga::front::spv::parse_u8_slice(&bytes, &options)
        .map_err(|error| ModuleError::Parse(error.to_string()))
}
