//! The objects' draws on the program runtime, prepared once and recorded every frame.
//!
//! A material's resources are lent to the runtime once, when the scene is uploaded: one
//! [`ImportTable`] per material holds the 576-byte material block, the surface maps, the sampler,
//! the contact heights and the impostor and wind textures, as the fixed material group binds
//! them. A [`PreparedSet`] is made from a table the first time a runtime needs it, and every
//! pipeline of that runtime whose set-1 layout equals the set's shares it.
//!
//! A prepared pipeline holds the bind groups it sets, so the runtime path prepares one pipeline
//! for each pair of a [`PipelineKey`] and a material that a scene draws. Recording a pair
//! costs what the fixed path spent on a pipeline change and a material change together: the
//! pass sets the pipeline once for the run of draws that share it
//! (`draw::object_runtime`'s pass adapter), and the material's group each time the material changes.
//! Nothing is built in a steady-state frame: pairs are made the first time a frame prepares for
//! a context, or the first time a draw needs one, and kept as long as the context lives.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use wgpu::{BindGroupLayout, Buffer, Sampler, TextureView};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    Import, ImportTable, Keepalive, PipelineDraw, PreparedPipeline, PreparedSet, ProgramRuntime,
};

use super::program::{Context, WeakContext, semantic, vertex_layout};
use crate::pipeline::PipelineKey;
use crate::terrain::program::WeakFamily;

fn named(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

/// The views one material's maps are bound through: albedo, normal, packed roughness and
/// metalness, ambient occlusion and self-illumination, then the relief depth, the impostor
/// borders and the wind field. A map the material lacks is the placeholder.
pub(crate) struct MaterialViews<'a> {
    pub maps: [&'a TextureView; 5],
    pub extra: [&'a TextureView; 3],
}

/// What the runtime is lent for one material, from the resources its group binds.
pub(crate) fn imports(
    views: &MaterialViews<'_>,
    uniform: &Buffer,
    sampler: &Sampler,
    heights: &Buffer,
) -> ImportTable {
    let mut table = ImportTable::new()
        .with(Import::buffer(named(semantic::MATERIAL), uniform.clone()))
        .with(Import::sampler(named(semantic::SAMPLER), sampler.clone()))
        .with(Import::buffer(
            named(semantic::CONTACT_HEIGHTS),
            heights.clone(),
        ));
    for (name, view) in [
        semantic::ALBEDO,
        semantic::NORMAL,
        semantic::ROUGH_METAL,
        semantic::AMBIENT_OCCLUSION,
        semantic::SELF_ILLUMINATION,
        semantic::IMPOSTOR_DEPTH,
        semantic::IMPOSTOR_BORDERS,
        semantic::WIND_FIELD,
    ]
    .into_iter()
    .zip(views.maps.into_iter().chain(views.extra))
    {
        table.insert(Import::texture(
            named(name),
            view.texture().clone(),
            view.clone(),
        ));
    }
    table
}

/// The family, material and set-1 layout a set is made for. Programs whose set-1 slots are
/// equal share one layout object, so one set serves all of them.
type SetKey = (u64, usize, BindGroupLayout);

/// The context, pipeline index and material a pipeline is made for.
type PairKey = (u64, usize, usize);

/// The material sets and pipelines a scene's objects have prepared.
#[derive(Debug)]
pub(crate) struct PreparedObjects {
    authority: SubmissionAuthority,
    /// What each material lends. Held by every set and pipeline made from it.
    tables: Vec<Arc<ImportTable>>,
    /// Sets are built against the runtime of a family, whose program fixes their layout, so they
    /// are kept per family and dropped when it is.
    sets: RefCell<BTreeMap<SetKey, (WeakFamily, PreparedSet)>>,
    /// Pipelines are kept per context and dropped when it is, so pipelines that a replay makes
    /// for one frame do not accumulate in the objects of a long-lived scene.
    pipelines: RefCell<BTreeMap<PairKey, (WeakContext, Rc<PreparedPipeline>)>>,
}

impl PreparedObjects {
    pub(crate) fn new(authority: SubmissionAuthority, tables: Vec<ImportTable>) -> Self {
        Self {
            authority,
            tables: tables.into_iter().map(Arc::new).collect(),
            sets: RefCell::default(),
            pipelines: RefCell::default(),
        }
    }

    /// Adds the materials an append made resident.
    pub(crate) fn extend(&mut self, tables: Vec<ImportTable>) {
        self.tables.extend(tables.into_iter().map(Arc::new));
    }

    /// Takes the resources a material is now bound to, and forgets the sets and pipelines made
    /// from the old ones.
    pub(crate) fn replace(&mut self, material: usize, table: ImportTable) {
        self.tables[material] = Arc::new(table);
        self.sets
            .get_mut()
            .retain(|(_, owner, _), _| *owner != material);
        self.pipelines
            .get_mut()
            .retain(|(_, _, owner), _| *owner != material);
    }

    /// The prepared pipeline of the pair, made on first use and found in a map afterwards.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or the pipeline.
    pub(crate) fn pipeline(
        &self,
        context: &Context,
        index: usize,
        material: usize,
    ) -> Result<Rc<PreparedPipeline>, ProgramError> {
        let key = (context.id(), index, material);
        if let Some((_, ready)) = self.pipelines.borrow().get(&key) {
            return Ok(ready.clone());
        }
        self.pipelines
            .borrow_mut()
            .retain(|_, (owner, _)| owner.alive());
        self.sets.borrow_mut().retain(|_, (owner, _)| owner.alive());
        let made = context
            .family()
            .with_objects((context.device(), &self.authority), |runtime| {
                self.prepare(runtime, context, (index, material))
            })?;
        let made = Rc::new(made);
        self.pipelines
            .borrow_mut()
            .insert(key, (context.downgrade(), made.clone()));
        Ok(made)
    }

    /// Prepares the material sets of every material for the runtime of `context`, so that the
    /// first frame finds them. A refusal is left for the draw that needs the pipeline to report.
    pub(crate) fn warm(&self, context: &Context) {
        if self.tables.is_empty() {
            return;
        }
        context
            .family()
            .with_objects((context.device(), &self.authority), |runtime| {
                let Ok(program) = context.program(runtime, PipelineKey::ALL[0]) else {
                    return;
                };
                for material in 0..self.tables.len() {
                    if self.set(runtime, context, &program, material).is_err() {
                        return;
                    }
                }
            });
    }

    fn prepare(
        &self,
        runtime: &mut ProgramRuntime,
        context: &Context,
        (index, material): (usize, usize),
    ) -> Result<PreparedPipeline, ProgramError> {
        let key = PipelineKey::ALL[index];
        let program = context.program(runtime, key)?;
        let set_key = self.set(runtime, context, &program, material)?;
        let sets = self.sets.borrow();
        let layout = vertex_layout(context.previous_transforms());
        let state = context.state(key);
        let owners: [Keepalive; 1] = [self.tables[material].clone()];
        runtime.prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &state,
                layout: &layout,
                bindings: &[],
                imports: &[],
                sets: &[&sets[&set_key].1],
                owners: &owners,
            },
            context.signature()?,
        )
    }

    /// Makes the set of `material` for the layout `program` has at set 1 if the family has none,
    /// and returns its key.
    fn set(
        &self,
        runtime: &mut ProgramRuntime,
        context: &Context,
        program: &worldrunner_render_program::runtime::GraphicsProgram,
        material: usize,
    ) -> Result<SetKey, ProgramError> {
        let layout = program
            .set_layout(1)
            .ok_or(ProgramError::UnknownSet { set: 1 })?
            .clone();
        let key = (context.family().id(), material, layout);
        let mut sets = self.sets.borrow_mut();
        if let std::collections::btree_map::Entry::Vacant(vacant) = sets.entry(key.clone()) {
            let table = &self.tables[material];
            let owners: [Keepalive; 1] = [table.clone()];
            vacant.insert((
                context.family().downgrade(),
                runtime.prepare_set(program, 1, table, owners.to_vec())?,
            ));
        }
        Ok(key)
    }
}
