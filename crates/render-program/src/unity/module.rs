//! A stage's prepared SPIR-V, its binding table and its Unity parameters.

use std::sync::Arc;

use worldrunner_unity_serialized::shader_parameters::{DescriptorSlot, VariantParameters};
use worldrunner_unity_serialized::shader_program::ProgramParameters;
pub use worldrunner_unity_serialized::spirv_prep::ResourceBinding;

use super::declare::{declarations, resource_layout};
use super::layout::LayoutError;
pub use crate::module::{ModuleError, Reflected, StageKind};
use crate::module::{StageDeclarations, StageSource};

/// The bit that names `kind` in a player variant's descriptor slots. Only the vertex and
/// fragment bits have been measured, so a compute stage has none.
#[must_use]
pub fn stage_bit(kind: StageKind) -> Option<u8> {
    match kind {
        StageKind::Vertex => Some(DescriptorSlot::VERTEX),
        StageKind::Fragment => Some(DescriptorSlot::FRAGMENT),
        StageKind::Compute => None,
    }
}

/// What describes the resources a stage binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageParameters {
    /// A sub program's parameters, which an editor-style serialized shader stores per stage.
    Program(ProgramParameters),
    /// A Unity 6 player variant's parameters, which describe every stage of the variant and
    /// name the stages that use each binding.
    Variant(VariantParameters),
}

impl From<ProgramParameters> for StageParameters {
    fn from(parameters: ProgramParameters) -> Self {
        Self::Program(parameters)
    }
}

impl From<VariantParameters> for StageParameters {
    fn from(parameters: VariantParameters) -> Self {
        Self::Variant(parameters)
    }
}

/// A prepared stage module: SPIR-V that naga reads, and where its resources are bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageModule {
    pub words: Vec<u32>,
    pub entry_point: String,
    pub kind: StageKind,
    pub bindings: Vec<ResourceBinding>,
}

/// A stage with its Unity parameters. It holds the parsed neutral stage; the parameters become
/// neutral declarations when a layout is built.
///
/// Cloning shares the parsed module.
#[derive(Clone)]
pub struct StageProgram {
    module: StageModule,
    parameters: StageParameters,
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
    pub fn new(
        module: StageModule,
        parameters: impl Into<StageParameters>,
    ) -> Result<Self, ModuleError> {
        let neutral = crate::module::StageModule {
            source: StageSource::SpirV(Arc::from(module.words.as_slice())),
            entry_point: module.entry_point.clone(),
            kind: module.kind,
        };
        let parsed = crate::module::StageProgram::new(neutral, StageDeclarations::default())?;
        Ok(Self {
            module,
            parameters: parameters.into(),
            parsed,
        })
    }

    #[must_use]
    pub fn module(&self) -> &StageModule {
        &self.module
    }

    #[must_use]
    pub fn parameters(&self) -> &StageParameters {
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
        let kind = self.module.kind;
        let declared = declarations(&self.parameters, &self.module.bindings, kind)?;
        let resources = resource_layout(&declared, self.parsed.reflected(), kind)?;
        Ok(self.parsed.with_declarations(StageDeclarations {
            resources,
            constants: Vec::new(),
        }))
    }
}
