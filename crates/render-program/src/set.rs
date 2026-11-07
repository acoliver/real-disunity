//! Bind group sets built once from imports and shared by every draw that uses them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::bind::{Imports, Sources};
use crate::binding::EngineSemantic;
use crate::error::ProgramError;
use crate::import::{Import, ImportedResource};
use crate::program::GraphicsProgram;
use crate::resources::{Keepalive, Pins};
use crate::runtime::ProgramRuntime;

/// Resources an owner lends, by the engine semantic they fill.
#[derive(Clone, Debug, Default)]
pub struct ImportTable {
    entries: BTreeMap<EngineSemantic, ImportedResource>,
}

impl ImportTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `import`, replacing and returning an earlier entry for the same semantic.
    pub fn insert(&mut self, import: Import) -> Option<ImportedResource> {
        self.entries.insert(import.semantic, import.resource)
    }

    #[must_use]
    pub fn with(mut self, import: Import) -> Self {
        self.insert(import);
        self
    }

    #[must_use]
    pub fn get(&self, semantic: &EngineSemantic) -> Option<&ImportedResource> {
        self.entries.get(semantic)
    }
}

/// A bind group built once for one set of a program, for any number of draws to bind.
///
/// Draws of any program whose slots at the set equal the ones it was built for share it: pass it
/// in [`ImportedDraw::sets`](crate::import::ImportedDraw::sets). A draw holds the group and the
/// owners given to [`prepare_set`](ProgramRuntime::prepare_set), and so does every frame that
/// retains the draw, so the set can be dropped once the draws are made.
#[derive(Debug)]
pub struct PreparedSet {
    pub(crate) set: u32,
    /// The runtime whose interned layout the group was built against.
    pub(crate) runtime: u64,
    /// The key of that layout, equal for programs whose slots at the set are equal.
    pub(crate) key: blake3::Hash,
    pub(crate) group: wgpu::BindGroup,
    /// Holds the owners. Draws hold a share of it for as long as they or a frame that retained
    /// them live.
    pub(crate) pins: Arc<Pins>,
}

impl PreparedSet {
    #[must_use]
    pub fn set(&self) -> u32 {
        self.set
    }
}

impl ProgramRuntime {
    /// Builds the bind group of `set` of `program` once, filling each binding from the entry of
    /// `table` that has its engine semantic. `owners` stay alive as long as a draw that binds
    /// the set, or a frame that retained such a draw, does.
    ///
    /// The table may hold entries for bindings this program does not use, so one table can serve
    /// every variant of a material. A binding the program does use with no entry is
    /// `MissingImport`, naming the semantic, set and binding. Sampler bindings that name no
    /// engine semantic get the runtime's sampler.
    ///
    /// # Errors
    /// Fails with `ExternalSetConflict` for a set the pass owner binds, with `UnknownSet` for a
    /// set with no bindings, with `MissingBinding` for a binding that is neither sourced from
    /// an engine semantic nor a sampler, with an import that is not the kind of resource its
    /// binding is, a buffer smaller than the shader needs, a lost device or a GPU validation
    /// error. Nothing is counted then.
    pub fn prepare_set(
        &mut self,
        program: &GraphicsProgram,
        set: u32,
        table: &ImportTable,
        owners: Vec<Keepalive>,
    ) -> Result<PreparedSet, ProgramError> {
        self.authority.check_device()?;
        let gpu = &program.gpu;
        if gpu.is_external(set) {
            return Err(ProgramError::ExternalSetConflict { set });
        }
        let index = usize::try_from(set).map_err(|_| ProgramError::UnknownSet { set })?;
        let (Some(Some(layout)), Some(Some(key))) =
            (gpu.groups.get(index), gpu.set_keys.get(index))
        else {
            return Err(ProgramError::UnknownSet { set });
        };
        let sources = Sources {
            bound: &[],
            snapshots: &[],
            imports: Imports::Table(table),
        };
        let group = self.build_group(gpu, (set, layout), sources)?;
        self.counters.set_prepared();
        Ok(PreparedSet {
            set,
            runtime: self.registry.runtime(),
            key: *key,
            group,
            pins: Arc::new(Pins::borrowing(&self.authority, Vec::new(), owners)),
        })
    }
}
