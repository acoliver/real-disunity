//! A stage's module, parsed and validated, with the bindings and constants it declares.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};

use crate::binding::ResourceLayout;
use crate::constants::BoundConstantLayout;
pub use crate::reflect::Reflected;
use crate::reflect::reflect;

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
}

impl StageSource {
    fn tag(&self) -> &'static str {
        match self {
            Self::SpirV(_) => "spirv",
        }
    }

    fn update(&self, hasher: &mut blake3::Hasher) {
        match self {
            Self::SpirV(words) => {
                for word in words.iter() {
                    hasher.update(&word.to_le_bytes());
                }
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

/// The resources and constant buffers a stage declares. A declaration is checked against the
/// resources the entry point uses when a program's layout is built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageDeclarations {
    pub resources: ResourceLayout,
    pub constants: Vec<BoundConstantLayout>,
}

/// What naga made of the module.
pub(crate) struct Parsed {
    pub(crate) module: naga::Module,
    pub(crate) resources: Vec<Reflected>,
}

/// A stage ready to build pipelines from: parsed, validated, with its declarations.
///
/// Cloning shares the parsed module.
#[derive(Clone)]
pub struct StageProgram {
    module: StageModule,
    declarations: StageDeclarations,
    parsed: Arc<Parsed>,
    key: blake3::Hash,
}

impl std::fmt::Debug for StageProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageProgram")
            .field("entry_point", &self.module.entry_point)
            .field("kind", &self.module.kind)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl StageProgram {
    /// Parses and validates `module` with naga, reading the resources its entry point uses.
    ///
    /// # Errors
    /// Fails with naga's parse or validation error, a missing entry point, or a resource that
    /// cannot be bound.
    pub fn new(module: StageModule, declarations: StageDeclarations) -> Result<Self, ModuleError> {
        let parsed = catch_unwind(AssertUnwindSafe(|| parse(&module)))
            .map_err(|payload| ModuleError::Panicked(panic_text(payload.as_ref())))??;
        let mut hasher = blake3::Hasher::new();
        hasher.update(module.source.tag().as_bytes());
        module.source.update(&mut hasher);
        hasher.update(module.entry_point.as_bytes());
        hasher.update(format!("{:?}", module.kind).as_bytes());
        Ok(Self {
            module,
            declarations,
            parsed: Arc::new(parsed),
            key: hasher.finalize(),
        })
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

    /// A hash of the source, entry point and stage, which identifies the compiled module.
    #[must_use]
    pub fn content_key(&self) -> blake3::Hash {
        self.key
    }

    /// The resources the entry point uses, by set and binding.
    #[must_use]
    pub fn reflected(&self) -> &[Reflected] {
        &self.parsed.resources
    }

    /// The module as naga parsed it.
    #[must_use]
    pub fn naga_module(&self) -> &naga::Module {
        &self.parsed.module
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

fn parse(stage: &StageModule) -> Result<Parsed, ModuleError> {
    let module = match &stage.source {
        StageSource::SpirV(words) => parse_spirv(words)?,
    };
    let info: ModuleInfo = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| ModuleError::Validate(format!("{:?}", error.into_inner())))?;
    let entry = module
        .entry_points
        .iter()
        .position(|e| e.name == stage.entry_point && e.stage == stage.kind.naga())
        .ok_or_else(|| ModuleError::EntryPointMissing {
            name: stage.entry_point.clone(),
            kind: stage.kind,
        })?;
    let resources = reflect(&module, &info, entry)?;
    Ok(Parsed { module, resources })
}

fn parse_spirv(words: &[u32]) -> Result<naga::Module, ModuleError> {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    // The default options flip the vertex position's Y, which turns the SPIR-V clip space
    // (Y down) into the one wgpu draws with; the crate's convention tests pin the result.
    let options = naga::front::spv::Options::default();
    naga::front::spv::parse_u8_slice(&bytes, &options)
        .map_err(|error| ModuleError::Parse(error.to_string()))
}
