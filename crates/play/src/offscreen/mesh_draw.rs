//! Mesh draws of a command buffer.
//!
//! A `DrawMesh` record draws one sub-mesh of a stored mesh with a matrix and a material's passes
//! into the targets bound at its place in the frame, with the view the script set up last. The
//! host draws it as a renderer list does one renderer: the record makes a renderer of the mesh
//! that carries the record's matrix as its per-object values and the record's property block as
//! its block (between the per-object values and the material), no material slots, and one
//! [`DrawList`] whose items draw the sub-mesh as the record's material, one item for each pass.
//! The mesh, its channels, the material's constants and textures and the render state of the pass
//! are then the ones a list draw gives them. The record's instance count goes to each item.

use std::collections::BTreeMap;
use std::sync::Arc;

use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    DotsBatch, DrawList, Instances, ListDraw, Record, Renderer,
};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{
    DrawBatch, MaterialId as WorldMaterialId, ShaderProperties,
};

use super::frame::{Mapper, Skip};
use super::ids;
use super::lists::{IMAGE_FRONT_FACE, list_pass_selector};
use super::properties::layer;

/// The layer of a mesh draw's renderer. No layer has this number, so no camera of the frame
/// culls the renderer in.
const NO_CAMERA_LAYER: u32 = 32;

/// What a mesh draw makes instances of.
enum Instanced<'a> {
    /// The sub-mesh repeated `instances` times with `matrix` and the record's own `block`.
    Count {
        matrix: &'a [f32; 16],
        block: Option<&'a ShaderProperties>,
        instances: u32,
    },
    /// The instances a shader reads from a window of a compute buffer.
    Batch(&'a DrawBatch),
}

/// The renderer a mesh draw makes, and the instances each of its passes draws.
struct Prepared {
    block: PropertyLayer,
    per_draw: PropertyLayer,
    instances: Instances,
}

/// The passes a draw of pass index `pass` makes: that one, or each of the shader's `passes` in
/// order for -1.
fn indexed_selectors(pass: i32, passes: usize) -> Result<Vec<PassSelector>, Skip> {
    let range = match usize::try_from(pass) {
        Ok(one) if one < passes => one..one + 1,
        Err(_) if passes > 0 => 0..passes,
        _ => return Err(Skip::NoSuchPass),
    };
    Ok(range.map(PassSelector::Index).collect())
}

impl Mapper<'_> {
    /// The record of mesh `mesh`, its sub-mesh `submesh` drawn with `matrix`, `material` and the
    /// record's own property `block`; `pass` picks the one pass drawn, or every pass in order when
    /// it is -1. Every drawn pass makes `instances` instances of the sub-mesh. A draw of none
    /// checks what any draw does and then makes no record, so the commands around it keep their
    /// order.
    pub(super) fn draw_mesh(
        &mut self,
        (mesh, submesh): (u64, u32),
        matrix: &[f32; 16],
        (material, pass): (WorldMaterialId, i32),
        (block, instances): (Option<&ShaderProperties>, u32),
    ) -> Result<Vec<Record>, Skip> {
        let counted = Instanced::Count {
            matrix,
            block,
            instances,
        };
        self.mesh_draw((mesh, submesh), (material, pass), &counted)
    }

    /// The record of mesh `mesh`, its sub-mesh `submesh` drawn as `material` for each instance
    /// `batch` lists, which the shader reads from the batch's window of its buffer. A buffer the
    /// host does not hold leaves the draw out; a batch that lists no instance draws nothing and
    /// makes no record.
    pub(super) fn draw_mesh_batch(
        &mut self,
        (mesh, submesh): (u64, u32),
        (material, pass): (WorldMaterialId, i32),
        batch: &DrawBatch,
    ) -> Result<Vec<Record>, Skip> {
        let records =
            self.mesh_draw((mesh, submesh), (material, pass), &Instanced::Batch(batch))?;
        if !records.is_empty() {
            self.note_batch(material, batch);
        }
        Ok(records)
    }

    /// Counts a batch draw that made a record by the shader that draws it, the pass tags it asks
    /// for and the properties the game gave it metadata words for, which is what the world's
    /// batch records leave unnamed.
    fn note_batch(&mut self, material: WorldMaterialId, batch: &DrawBatch) {
        let shader = self
            .snapshot
            .materials
            .get(&material)
            .and_then(|held| self.snapshot.shaders.get(&held.shader))
            .map_or("unknown shader", |shader| shader.name.as_str());
        let names: Vec<&str> = batch
            .metadata
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        let note = format!(
            "batch draw: {shader} [{}] properties {}",
            batch.pass_tags.join("+"),
            names.join(","),
        );
        self.report.note_many(&note, 1);
    }

    fn mesh_draw(
        &mut self,
        (mesh, submesh): (u64, u32),
        (material, pass): (WorldMaterialId, i32),
        instanced: &Instanced<'_>,
    ) -> Result<Vec<Record>, Skip> {
        let (target, raster) = self.draw_target()?;
        self.check_sub_mesh(mesh, submesh)?;
        let (named, passes) = self.material_and_passes(material)?;
        let selectors = match instanced {
            Instanced::Batch(DrawBatch { pass_tags, .. }) if !pass_tags.is_empty() => {
                vec![self.tagged_selector(material, pass_tags)?]
            }
            _ => indexed_selectors(pass, passes)?,
        };
        let (values, _) = self.current_view()?;
        let Some(prepared) = self.prepare(instanced)? else {
            return Ok(Vec::new());
        };
        let renderer = ids::mesh_draw_renderer(self.script.mesh_draws);
        self.script.mesh_draws += 1;
        self.frame.renderers.insert(
            renderer,
            Renderer {
                mesh: ids::mesh(mesh),
                materials: Vec::new(),
                layer: NO_CAMERA_LAYER,
                block: prepared.block,
                slot_blocks: BTreeMap::new(),
                per_draw: prepared.per_draw,
            },
        );
        let slot = usize::try_from(submesh).unwrap_or(usize::MAX);
        Ok(vec![Record::Draws(Arc::new(DrawList {
            raster,
            target,
            values,
            front_face: IMAGE_FRONT_FACE,
            draws: selectors
                .into_iter()
                .map(|pass| ListDraw {
                    renderer,
                    slot,
                    material: Some(named),
                    pass,
                    instances: prepared.instances.clone(),
                    per_draw: PropertyLayer::default(),
                })
                .collect(),
        }))])
    }

    /// The pass a batch draw makes for the `LightMode` values `tags`, which a renderer list that
    /// names them draws: the first pass with the first tag any pass of the material's shader has.
    fn tagged_selector(
        &self,
        material: WorldMaterialId,
        tags: &[String],
    ) -> Result<PassSelector, Skip> {
        let info = self
            .snapshot
            .materials
            .get(&material)
            .and_then(|held| self.available.shaders.get(&held.shader))
            .ok_or(Skip::ShaderUnavailable)?;
        info.list_tag(tags)
            .map(|_| list_pass_selector(tags))
            .ok_or(Skip::NoTaggedPass)
    }

    /// The renderer values and instances of a draw, or `None` for a draw of no instance.
    fn prepare(&mut self, instanced: &Instanced<'_>) -> Result<Option<Prepared>, Skip> {
        match instanced {
            Instanced::Count { instances: 0, .. } => Ok(None),
            Instanced::Count {
                matrix,
                block,
                instances,
            } => Ok(Some(Prepared {
                block: block.map_or_else(PropertyLayer::default, |block| {
                    layer(block, &mut self.textures)
                }),
                per_draw: self.layerless_draw(matrix),
                instances: Instances::Count(*instances),
            })),
            Instanced::Batch(batch) => {
                let buffer = *self
                    .available
                    .buffers
                    .get(&batch.buffer)
                    .ok_or(Skip::BufferUnavailable)?;
                let end = batch
                    .offset
                    .checked_add(batch.size)
                    .ok_or(Skip::BufferUnavailable)?;
                Ok((!batch.visible.is_empty()).then(|| Prepared {
                    block: PropertyLayer::default(),
                    per_draw: self.layerless_batch_draw(),
                    instances: Instances::Dots(DotsBatch {
                        buffer,
                        window: batch.offset..end,
                        metadata: batch.metadata.clone(),
                        visible: batch.visible.clone(),
                    }),
                }))
            }
        }
    }

    /// Whether sub-mesh `submesh` of mesh `mesh` has triangles to draw: the host holds the mesh,
    /// the mesh has such a sub-mesh, and the sub-mesh has indices.
    fn check_sub_mesh(&self, mesh: u64, submesh: u32) -> Result<(), Skip> {
        let available = self.available;
        if available.empty_meshes.contains(&mesh) {
            return Err(Skip::EmptyMesh);
        }
        if !available.meshes.contains(&mesh) {
            return Err(Skip::MeshUnavailable);
        }
        let counts = available
            .sub_meshes
            .get(&mesh)
            .map_or(&[][..], Vec::as_slice);
        match usize::try_from(submesh).ok().and_then(|at| counts.get(at)) {
            None => Err(Skip::NoSuchSubmesh),
            Some(0) => Err(Skip::EmptyMesh),
            Some(_) => Ok(()),
        }
    }
}
