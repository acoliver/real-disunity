//! Programs a caller registers by description and the handles that name them.
//!
//! A registered program is immutable. Registration parses and validates every stage, builds the
//! layouts of every variant and checks the constant layouts, so a program that registers can be
//! drawn. A failed registration publishes no handle.

use std::collections::HashMap;
use std::sync::Arc;

use crate::binding::{BindingAddress, ResourceBinding};
use crate::constants::{BoundConstantLayout, pack_constants};
use crate::error::ProgramError;
use crate::layout::LayoutError;
use crate::material::{MaterialEntry, MaterialHandle};
use crate::module::{StageDeclarations, StageKind, StageModule, StageProgram, StageSource};
use crate::program::GraphicsProgram;
use crate::runtime::ProgramRuntime;
use crate::schema::Schema;
use crate::state::RenderState;
use crate::variant::{FeatureSet, UnknownFeatures, Variants};

/// A registered program. A handle belongs to the runtime that made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramHandle {
    pub(crate) runtime: u64,
    pub(crate) id: u64,
}

/// One stage: its code and the resources and constants it declares.
#[derive(Clone, Debug)]
pub struct StageDesc {
    pub module: StageModule,
    pub declarations: StageDeclarations,
    pub overrides: Vec<(String, f64)>,
}

impl StageDesc {
    /// The stage with pipeline-overridable constants set, as
    /// [`StageProgram::with_overrides`] does when the program is registered: they replace any
    /// the description had. Registration refuses, by name, a constant the module does not
    /// declare and a value of the wrong type.
    #[must_use]
    pub fn with_overrides(mut self, overrides: &[(&str, f64)]) -> Self {
        self.overrides = overrides
            .iter()
            .map(|&(name, value)| (name.to_owned(), value))
            .collect();
        self
    }

    /// The stage parsed and validated, with its overrides applied. A source that a stage in
    /// `known` was made from is not parsed again: the stage is a sibling of it. The stage is added
    /// to `known`.
    fn program(self, known: &mut Vec<StageProgram>) -> Result<StageProgram, ProgramError> {
        let overrides: Vec<(&str, f64)> = self
            .overrides
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        let parsed = known
            .iter()
            .find(|stage| stage.module().source == self.module.source)
            .cloned();
        let stage = match parsed {
            Some(parsed) => parsed
                .sibling(&self.module.entry_point, self.module.kind)?
                .with_declarations(self.declarations),
            None => StageProgram::new(self.module, self.declarations)?,
        };
        known.push(stage.clone());
        Ok(stage.with_overrides(&overrides)?)
    }

    /// A stage whose code is WGSL text.
    #[must_use]
    pub fn wgsl(
        source: impl Into<Arc<str>>,
        entry_point: &str,
        kind: StageKind,
        declarations: StageDeclarations,
    ) -> Self {
        Self {
            module: StageModule {
                source: StageSource::Wgsl(source.into()),
                entry_point: entry_point.to_owned(),
                kind,
            },
            declarations,
            overrides: Vec::new(),
        }
    }

    /// A stage whose code is SPIR-V words.
    #[must_use]
    pub fn spirv(
        words: impl Into<Arc<[u32]>>,
        entry_point: &str,
        kind: StageKind,
        declarations: StageDeclarations,
    ) -> Self {
        Self {
            module: StageModule {
                source: StageSource::SpirV(words.into()),
                entry_point: entry_point.to_owned(),
                kind,
            },
            declarations,
            overrides: Vec::new(),
        }
    }
}

/// The vertex and fragment stages a material runs when exactly `features` are enabled.
#[derive(Clone, Debug)]
pub struct VariantDesc {
    pub features: FeatureSet,
    pub vertex: StageDesc,
    pub fragment: StageDesc,
}

/// A program: its fixed-function state and the variants a feature set chooses between. The
/// features the program declares are the ones its variants use.
#[derive(Clone, Debug)]
pub struct ProgramDesc {
    pub label: String,
    pub state: RenderState,
    pub variants: Vec<VariantDesc>,
}

impl ProgramDesc {
    /// A program with one variant, which no feature selects.
    #[must_use]
    pub fn single(label: &str, state: RenderState, vertex: StageDesc, fragment: StageDesc) -> Self {
        Self {
            label: label.to_owned(),
            state,
            variants: vec![VariantDesc {
                features: FeatureSet::new(),
                vertex,
                fragment,
            }],
        }
    }
}

/// One variant as registered: the program and the declarations its draws are resolved from.
pub(crate) struct Variant {
    pub(crate) program: GraphicsProgram,
    /// The constant buffers of both stages, one per binding address.
    pub(crate) constants: Vec<BoundConstantLayout>,
    /// The resource bindings of both stages, one per binding address.
    pub(crate) resources: Vec<ResourceBinding>,
}

pub(crate) struct Registered {
    pub(crate) label: String,
    pub(crate) state: RenderState,
    pub(crate) variants: Variants<Variant>,
    pub(crate) schema: Schema,
}

/// The registered programs and materials of one runtime.
pub(crate) struct Registry {
    runtime: u64,
    next: u64,
    programs: HashMap<u64, Arc<Registered>>,
    materials: HashMap<u64, MaterialEntry>,
}

impl Registry {
    pub(crate) fn new(runtime: u64) -> Self {
        Self {
            runtime,
            next: 0,
            programs: HashMap::new(),
            materials: HashMap::new(),
        }
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub(crate) fn runtime(&self) -> u64 {
        self.runtime
    }

    pub(crate) fn add_program(&mut self, program: Registered) -> ProgramHandle {
        let id = self.id();
        self.programs.insert(id, Arc::new(program));
        ProgramHandle {
            runtime: self.runtime,
            id,
        }
    }

    pub(crate) fn program(&self, handle: ProgramHandle) -> Result<&Arc<Registered>, ProgramError> {
        self.programs
            .get(&handle.id)
            .filter(|_| handle.runtime == self.runtime)
            .ok_or(ProgramError::UnknownHandle { kind: "program" })
    }

    pub(crate) fn add_material(&mut self, entry: MaterialEntry) -> MaterialHandle {
        let id = self.id();
        self.materials.insert(id, entry);
        MaterialHandle {
            runtime: self.runtime,
            id,
        }
    }

    pub(crate) fn material(&self, handle: MaterialHandle) -> Result<&MaterialEntry, ProgramError> {
        self.materials
            .get(&handle.id)
            .filter(|_| handle.runtime == self.runtime)
            .ok_or(ProgramError::UnknownHandle { kind: "material" })
    }

    pub(crate) fn material_mut(
        &mut self,
        handle: MaterialHandle,
    ) -> Result<&mut MaterialEntry, ProgramError> {
        self.materials
            .get_mut(&handle.id)
            .filter(|_| handle.runtime == self.runtime)
            .ok_or(ProgramError::UnknownHandle { kind: "material" })
    }

    pub(crate) fn remove_material(&mut self, handle: MaterialHandle) -> bool {
        handle.runtime == self.runtime && self.materials.remove(&handle.id).is_some()
    }
}

impl Variant {
    fn new(program: GraphicsProgram, stages: [&StageProgram; 2]) -> Result<Self, ProgramError> {
        let mut constants: Vec<BoundConstantLayout> = Vec::new();
        let mut resources: Vec<ResourceBinding> = Vec::new();
        for stage in stages {
            let declarations = stage.declarations();
            for layout in &declarations.constants {
                add_constants(&mut constants, layout)?;
            }
            for binding in &declarations.resources.bindings {
                if !resources.iter().any(|known| known.at == binding.at) {
                    resources.push(binding.clone());
                }
            }
        }
        for layout in &constants {
            // An empty set of properties packs every member as missing, which still refuses a
            // member the packer cannot write or one that lies outside its buffer.
            pack_constants(&layout.layout, &[], 0)?;
        }
        Ok(Self {
            program,
            constants,
            resources,
        })
    }
}

/// Adds `layout`, which both stages may declare when they declare it identically.
fn add_constants(
    known: &mut Vec<BoundConstantLayout>,
    layout: &BoundConstantLayout,
) -> Result<(), ProgramError> {
    match known.iter().find(|have| have.at == layout.at) {
        None => known.push(layout.clone()),
        Some(have) if have == layout => {}
        Some(_) => return Err(differ(layout.at)),
    }
    Ok(())
}

fn differ(at: BindingAddress) -> ProgramError {
    LayoutError::Declaration {
        reason: format!(
            "the stages declare different constant layouts at set {} binding {}",
            at.set, at.binding
        ),
    }
    .into()
}

impl ProgramRuntime {
    /// The program of the variant that exactly `features` select, for draws over lent
    /// resources (`prepare_imported_draw`), which take a program rather than a material. The
    /// program is the one registration built, so resolving it builds nothing.
    ///
    /// # Errors
    /// Fails with `UnknownHandle` for a handle of another runtime, with
    /// `VariantError::UnknownFeature` for a feature no variant differs by and with
    /// `VariantError::NoMatch` when no variant has the set.
    pub fn variant_program(
        &self,
        handle: ProgramHandle,
        features: &FeatureSet,
    ) -> Result<GraphicsProgram, ProgramError> {
        let registered = self.registry.program(handle)?;
        let variant = registered
            .variants
            .select(features, UnknownFeatures::Fail)?;
        Ok(variant.program.clone())
    }

    /// Registers a program: parses every stage with naga, checks the declarations against what
    /// the entry points use, builds each variant's layouts and returns a handle for creating
    /// materials. The state must be one a draw supports.
    ///
    /// # Errors
    /// Fails with a named error when the program has no variants, a stage does not parse or
    /// validate, a declaration does not match its stage, two stages declare different
    /// constants at one address, two variants have one feature set or the state is refused.
    /// Nothing is registered then.
    pub fn register_program(&mut self, desc: ProgramDesc) -> Result<ProgramHandle, ProgramError> {
        if desc.variants.is_empty() {
            return Err(ProgramError::NoVariants {
                program: desc.label,
            });
        }
        desc.state.check_drawable()?;
        let declared: FeatureSet = desc
            .variants
            .iter()
            .flat_map(|variant| variant.features.iter().cloned())
            .collect();
        let mut built = Vec::new();
        let mut parsed = Vec::new();
        for variant in desc.variants {
            let vertex = variant.vertex.program(&mut parsed)?;
            let fragment = variant.fragment.program(&mut parsed)?;
            let program = self.graphics_program(&vertex, &fragment)?;
            built.push((
                variant.features,
                Variant::new(program, [&vertex, &fragment])?,
            ));
        }
        let schema = Schema::of(
            &desc.label,
            &built.iter().map(|(_, v)| v).collect::<Vec<_>>(),
        )?;
        let variants = Variants::new(declared, built)?;
        Ok(self.registry.add_program(Registered {
            label: desc.label,
            state: desc.state,
            variants,
            schema,
        }))
    }
}
