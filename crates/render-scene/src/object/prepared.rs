//! The objects' draws on the program runtime, prepared once and recorded every frame.
//!
//! A material's resources are lent to the runtime once, when the scene is uploaded: one
//! [`ImportTable`] per material holds the 576-byte material block, the surface maps, the sampler,
//! the contact heights and the impostor and wind textures. A
//! [`PreparedSet`] is made from a table the first time a runtime needs it, and every pipeline of
//! that runtime whose set-1 layout equals the set's shares it.
//!
//! A prepared pipeline holds the bind groups it sets, so the runtime path prepares one pipeline
//! for each pair of a [`PipelineKey`] and a material that a scene draws. Recording a pair
//! costs a pipeline change and a material change together: the pass sets the pipeline once for
//! the run of draws that share it (`draw::object_runtime`'s pass adapter), and the material's
//! group each time the material changes.
//! Nothing is built in a steady-state frame: pairs are made the first time a frame prepares for
//! a context, or the first time a draw needs one, and kept as long as the context lives.

#[cfg(test)]
mod test_hooks;
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

use super::casting;
use super::layout::{GeometryLayout, vertex_layout};
use super::program::{Context, WeakContext, caster_key, semantic};
use crate::pipeline::{MATERIAL_UNIFORM_BYTES, PipelineKey};
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

/// The semantics the views of [`MaterialViews`] are lent under, in the order of its `maps` and
/// then its `extra`.
const MAP_SEMANTICS: [&str; 8] = [
    semantic::ALBEDO,
    semantic::NORMAL,
    semantic::ROUGH_METAL,
    semantic::AMBIENT_OCCLUSION,
    semantic::SELF_ILLUMINATION,
    semantic::IMPOSTOR_DEPTH,
    semantic::IMPOSTOR_BORDERS,
    semantic::WIND_FIELD,
];

/// What the runtime is lent for one material: its views, its uniform block, its sampler and the
/// contact heights.
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
    for (name, view) in MAP_SEMANTICS
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

/// A material as the runtime is lent it: what it binds and the bytes of the uniform block in the
/// buffer it binds, which tell the shadow casters which materials they read alike.
pub(crate) struct LentMaterial {
    pub(crate) table: ImportTable,
    pub(crate) uniform: [u8; MATERIAL_UNIFORM_BYTES],
}

/// The family, material and set-1 layout a set is made for. Programs whose set-1 slots are
/// equal share one layout object, so one set serves all of them.
type SetKey = (u64, usize, BindGroupLayout);

/// The context, pipeline index, material and geometry layout a pipeline is made for.
type PairKey = (u64, usize, usize, GeometryLayout);

/// The material sets and pipelines a scene's objects have prepared.
#[derive(Debug)]
pub(crate) struct PreparedObjects {
    authority: SubmissionAuthority,
    /// What each material lends. Held by every set and pipeline made from it.
    tables: Vec<Arc<ImportTable>>,
    /// The bytes of each material's uniform block that a shadow caster reads.
    caster_reads: Vec<Box<[u8]>>,
    /// The material whose group an unmasked caster binds for each material.
    caster_classes: Vec<usize>,
    /// The scene palette every table lends to the programs of skinned geometry.
    palette: Option<Buffer>,
    /// Sets are built against the runtime of a family, whose program fixes their layout, so they
    /// are kept per family and dropped when it is.
    sets: RefCell<BTreeMap<SetKey, (WeakFamily, PreparedSet)>>,
    /// Pipelines are kept per context and dropped when it is, so pipelines that a replay makes
    /// for one frame do not accumulate in the objects of a long-lived scene.
    pipelines: RefCell<BTreeMap<PairKey, (WeakContext, Rc<PreparedPipeline>)>>,
    /// The pair lists `prepare_pairs` found every pipeline of for a context, so a frame whose
    /// list is the one it prepared last looks up none of them.
    prepared: RefCell<Vec<PreparedList>>,
    /// How many times `pipeline` was asked for a pair.
    #[cfg(test)]
    lookups: std::cell::Cell<usize>,
}

/// The pipeline index, material and vertex format of a pipeline a scene draws with.
pub(crate) type Pair = (usize, usize, GeometryLayout);

/// A list of pairs whose pipelines exist for one context.
#[derive(Debug)]
struct PreparedList {
    id: u64,
    context: WeakContext,
    pairs: Rc<[Pair]>,
}

impl PreparedObjects {
    pub(crate) fn new(
        authority: SubmissionAuthority,
        materials: Vec<LentMaterial>,
        palette: Option<Buffer>,
    ) -> Self {
        let mut objects = Self {
            authority,
            tables: Vec::new(),
            caster_reads: Vec::new(),
            caster_classes: Vec::new(),
            palette,
            sets: RefCell::default(),
            pipelines: RefCell::default(),
            prepared: RefCell::default(),
            #[cfg(test)]
            lookups: std::cell::Cell::default(),
        };
        objects.extend(materials);
        objects
    }

    /// Lends the scene palette through `table`, so that a skinned program of the material finds
    /// it. A static program does not declare the binding and ignores the entry.
    fn lend(&self, mut table: ImportTable) -> Arc<ImportTable> {
        if let Some(palette) = &self.palette {
            table.insert(Import::buffer(
                named(semantic::SKIN_PALETTE),
                palette.clone(),
            ));
        }
        Arc::new(table)
    }

    /// Adds the materials an append made resident.
    pub(crate) fn extend(&mut self, materials: Vec<LentMaterial>) {
        for material in materials {
            self.caster_reads.push(casting::read(&material.uniform));
            let lent = self.lend(material.table);
            self.tables.push(lent);
        }
        self.caster_classes = casting::classes(&self.caster_reads, &self.tables);
    }

    /// Takes the resources a material is now bound to, and forgets the sets and pipelines made
    /// from the old ones.
    pub(crate) fn replace(&mut self, material: usize, table: ImportTable) {
        self.tables[material] = self.lend(table);
        self.caster_classes = casting::classes(&self.caster_reads, &self.tables);
        self.sets
            .get_mut()
            .retain(|(_, owner, _), _| *owner != material);
        self.pipelines
            .get_mut()
            .retain(|(_, _, owner, _), _| *owner != material);
        self.prepared.get_mut().clear();
    }

    /// The pipeline index and material that a draw of `pipeline` and `material` records with in
    /// the passes of `context`. A forward pass records every draw as it is. A caster records the
    /// pipeline of the key it casts as, and an unmasked draw the material it reads as, so that
    /// materials the caster reads alike share one group and sort into one run.
    pub(crate) fn recorded_as(
        &self,
        context: &Context,
        pipeline: usize,
        material: usize,
    ) -> (usize, usize) {
        if !context.is_caster() {
            return (pipeline, material);
        }
        let key = caster_key(PipelineKey::ALL[pipeline]);
        let material = if key.masked {
            material
        } else {
            self.caster_classes[material]
        };
        (key.index(), material)
    }

    /// The prepared pipeline of the pair, made on first use and found in a map afterwards.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or the pipeline.
    pub(crate) fn pipeline(
        &self,
        context: &Context,
        (index, material): (usize, usize),
        geometry: GeometryLayout,
    ) -> Result<Rc<PreparedPipeline>, ProgramError> {
        #[cfg(test)]
        self.lookups.set(self.lookups.get() + 1);
        let key = (context.id(), index, material, geometry);
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
                self.prepare(runtime, context, (index, material), geometry)
            })?;
        let made = Rc::new(made);
        self.pipelines
            .borrow_mut()
            .insert(key, (context.downgrade(), made.clone()));
        Ok(made)
    }

    /// Makes the pipeline of every pair in `pairs` for `context`, unless this very list was made
    /// for it already: a pipeline stays for as long as its context lives and until its
    /// material's resources are replaced, which forgets the lists.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or a pipeline; the list is then not
    /// remembered.
    pub(crate) fn prepare_pairs(
        &self,
        context: &Context,
        pairs: &Rc<[Pair]>,
    ) -> Result<(), ProgramError> {
        let id = context.id();
        if self
            .prepared
            .borrow()
            .iter()
            .any(|list| list.id == id && Rc::ptr_eq(&list.pairs, pairs))
        {
            return Ok(());
        }
        for &(index, material, geometry) in pairs.iter() {
            let recorded = self.recorded_as(context, index, material);
            self.pipeline(context, recorded, geometry)?;
        }
        let mut prepared = self.prepared.borrow_mut();
        prepared.retain(|list| list.id != id && list.context.alive());
        prepared.push(PreparedList {
            id,
            context: context.downgrade(),
            pairs: Rc::clone(pairs),
        });
        Ok(())
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
                let Ok(program) =
                    context.program(runtime, PipelineKey::ALL[0], GeometryLayout::Static)
                else {
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
        geometry: GeometryLayout,
    ) -> Result<PreparedPipeline, ProgramError> {
        let key = PipelineKey::ALL[index];
        let program = context.program(runtime, key, geometry)?;
        let set_key = self.set(runtime, context, &program, material)?;
        let sets = self.sets.borrow();
        let layout = vertex_layout(context.previous_transforms(), geometry);
        let state = context.state(key);
        let owners: [Keepalive; 1] = [self.tables[material].clone()];
        runtime.prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &state,
                layout,
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
