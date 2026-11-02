//! A stage's prepared SPIR-V, its binding table and its Unity parameters.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};
use worldrunner_unity_serialized::shader_program::ProgramParameters;

use crate::reflect::{Reflected, reflect};

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

/// Where one resource of a prepared module is bound: the same data as the importer's
/// `ResourceBinding`, which this crate cannot depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleBinding {
    pub set: u32,
    /// The binding in Unity's module, which parameters refer to.
    pub unity_binding: u32,
    /// The binding in the prepared module; for a split combined sampler, the image's.
    pub binding: u32,
    /// For a split combined sampler, the sampler's binding in the prepared module.
    pub sampler_binding: Option<u32>,
}

/// A prepared stage module: SPIR-V that naga reads, and where its resources are bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageModule {
    pub words: Vec<u32>,
    pub entry_point: String,
    pub kind: StageKind,
    pub bindings: Vec<ModuleBinding>,
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

/// What naga made of the module.
pub(crate) struct Parsed {
    pub(crate) module: naga::Module,
    pub(crate) resources: Vec<Reflected>,
}

/// A stage ready to build pipelines from: parsed, validated, with its parameters.
///
/// Cloning shares the parsed module.
#[derive(Clone)]
pub struct StageProgram {
    module: StageModule,
    parameters: ProgramParameters,
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
    pub fn new(module: StageModule, parameters: ProgramParameters) -> Result<Self, ModuleError> {
        let words = &module.words;
        let parsed = catch_unwind(AssertUnwindSafe(|| parse(words, &module)))
            .map_err(|payload| ModuleError::Panicked(panic_text(payload.as_ref())))??;
        let mut hasher = blake3::Hasher::new();
        for word in &module.words {
            hasher.update(&word.to_le_bytes());
        }
        hasher.update(module.entry_point.as_bytes());
        hasher.update(format!("{:?}", module.kind).as_bytes());
        Ok(Self {
            module,
            parameters,
            parsed: Arc::new(parsed),
            key: hasher.finalize(),
        })
    }

    #[must_use]
    pub fn module(&self) -> &StageModule {
        &self.module
    }

    #[must_use]
    pub fn parameters(&self) -> &ProgramParameters {
        &self.parameters
    }

    /// A hash of the SPIR-V, entry point and stage, which identifies the compiled module.
    #[must_use]
    pub fn content_key(&self) -> blake3::Hash {
        self.key
    }

    /// The resources the entry point uses, by prepared set and binding.
    #[must_use]
    pub fn resources(&self) -> &[Reflected] {
        &self.parsed.resources
    }

    /// The module as naga parsed it from the SPIR-V.
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

fn parse(words: &[u32], stage: &StageModule) -> Result<Parsed, ModuleError> {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    // The default options flip the vertex position's Y, which turns the SPIR-V clip space
    // (Y down) into the one wgpu draws with; the crate's convention tests pin the result.
    let options = naga::front::spv::Options::default();
    let module = naga::front::spv::parse_u8_slice(&bytes, &options)
        .map_err(|error| ModuleError::Parse(error.to_string()))?;
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
