//! A stage's prepared SPIR-V, its binding table and its Unity parameters.

use std::sync::Arc;

use worldrunner_unity_serialized::shader_program::ProgramParameters;

use super::declare::{declarations, resource_layout};
use super::layout::LayoutError;
pub use crate::module::{ModuleError, Reflected, StageKind};
use crate::module::{StageDeclarations, StageSource};

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

/// A stage with its Unity parameters. It holds the parsed neutral stage; the parameters become
/// neutral declarations when a layout is built.
///
/// Cloning shares the parsed module.
#[derive(Clone)]
pub struct StageProgram {
    module: StageModule,
    parameters: ProgramParameters,
    parsed: crate::module::StageProgram,
}

impl std::fmt::Debug for StageProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageProgram")
            .field("parsed", &self.parsed)
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
        let neutral = crate::module::StageModule {
            source: StageSource::SpirV(Arc::from(module.words.as_slice())),
            entry_point: module.entry_point.clone(),
            kind: module.kind,
        };
        let parsed = crate::module::StageProgram::new(neutral, StageDeclarations::default())?;
        Ok(Self {
            module,
            parameters,
            parsed,
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
        self.parsed.content_key()
    }

    /// The resources the entry point uses, by prepared set and binding.
    #[must_use]
    pub fn resources(&self) -> &[Reflected] {
        self.parsed.reflected()
    }

    /// The module as naga parsed it from the SPIR-V.
    #[must_use]
    pub fn naga_module(&self) -> &naga::Module {
        self.parsed.naga_module()
    }

    /// The neutral stage: the parsed module with the resources its parameters declare.
    ///
    /// # Errors
    /// Fails on a parameter kind that is not bound, a parameter outside the binding table, or
    /// a parameter of another kind than the module's resource at its binding.
    pub fn neutral(&self) -> Result<crate::module::StageProgram, LayoutError> {
        let declared = declarations(&self.parameters, &self.module.bindings)?;
        let resources = resource_layout(
            &declared,
            self.parsed.reflected(),
            self.module.kind.visibility(),
        )?;
        Ok(self.parsed.with_declarations(StageDeclarations {
            resources,
            constants: Vec::new(),
        }))
    }
}
