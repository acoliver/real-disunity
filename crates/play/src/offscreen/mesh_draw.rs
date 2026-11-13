//! Mesh draws of a command buffer.
//!
//! A `DrawMesh` record draws one sub-mesh of a stored mesh with a matrix and a material's passes
//! into the targets bound at its place in the frame, with the view the script set up last. The
//! host draws it as a renderer list does one renderer: the record makes a renderer of the mesh
//! that carries the record's matrix as its per-object values and the record's property block as
//! its block (between the per-object values and the material), no material slots, and one
//! [`DrawList`] whose items draw the sub-mesh as the record's material, one item for each pass.
//! The mesh, its channels, the material's constants and textures and the render state of the pass
//! are then the ones a list draw gives them.

use std::sync::Arc;

use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{DrawList, ListDraw, Record, Renderer};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{MaterialId as WorldMaterialId, ShaderProperties};

use super::builtins::draw_layer;
use super::frame::{Mapper, Skip};
use super::ids;
use super::lists::IMAGE_FRONT_FACE;
use super::properties::layer;

/// The layer of a mesh draw's renderer. No layer has this number, so no camera of the frame
/// culls the renderer in.
const NO_CAMERA_LAYER: u32 = 32;

impl Mapper<'_> {
    /// The record of mesh `mesh`, its sub-mesh `submesh` drawn with `matrix`, `material` and the
    /// record's own property `block`; `pass` picks the one pass drawn, or every pass in order when
    /// it is -1.
    pub(super) fn draw_mesh(
        &mut self,
        (mesh, submesh): (u64, u32),
        matrix: &[f32; 16],
        (material, pass): (WorldMaterialId, i32),
        block: Option<&ShaderProperties>,
    ) -> Result<Vec<Record>, Skip> {
        let target = self.script.target.ok_or(Skip::UnboundTarget)?;
        self.check_sub_mesh(mesh, submesh)?;
        let (named, passes) = self.material_and_passes(material)?;
        let passes = match usize::try_from(pass) {
            Ok(one) if one < passes => one..one + 1,
            Err(_) if passes > 0 => 0..passes,
            _ => return Err(Skip::NoSuchPass),
        };
        let (values, _) = self.current_view()?;
        let renderer = ids::mesh_draw_renderer(self.script.mesh_draws);
        self.script.mesh_draws += 1;
        self.frame.renderers.insert(
            renderer,
            Renderer {
                mesh: ids::mesh(mesh),
                materials: Vec::new(),
                layer: NO_CAMERA_LAYER,
                block: block.map_or_else(PropertyLayer::default, |block| {
                    layer(block, &mut self.textures)
                }),
                per_draw: draw_layer(matrix),
            },
        );
        let slot = usize::try_from(submesh).unwrap_or(usize::MAX);
        Ok(vec![Record::Draws(Arc::new(DrawList {
            target,
            values,
            front_face: IMAGE_FRONT_FACE,
            draws: passes
                .map(|pass| ListDraw {
                    renderer,
                    slot,
                    material: Some(named),
                    pass: PassSelector::Index(pass),
                })
                .collect(),
        }))])
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
