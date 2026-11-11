//! Procedural draws of a command buffer.
//!
//! A `DrawProcedural` record draws a material's passes over vertices the shader makes from the
//! vertex and instance index, into the targets bound at its place in the frame, with the view
//! the script set up last and the record's matrix as its per-draw values. It becomes one
//! [`ProceduralDraw`] record, with one pass for each pass drawn. A draw of zero vertices or zero
//! instances checks its target, material and pass as any draw does and then makes no record, so
//! the commands around it keep their order and nothing is drawn.

use std::sync::Arc;

use worldrunner_render_program::unity::offscreen::frame::{ProceduralDraw, Record};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::{DrawTopology, MaterialId as WorldMaterialId};

use super::builtins::draw_layer;
use super::frame::{Mapper, Skip};
use super::lists::IMAGE_FRONT_FACE;

impl Mapper<'_> {
    /// The record of a procedural draw with `matrix`, `material` and `pass` (-1 for every pass in
    /// order), joining `vertices` vertices by `topology`, `instances` times.
    pub(super) fn draw_procedural(
        &mut self,
        matrix: &[f32; 16],
        (material, pass): (WorldMaterialId, i32),
        topology: DrawTopology,
        (vertices, instances): (u32, u32),
    ) -> Result<Vec<Record>, Skip> {
        let target = self.script.target.ok_or(Skip::UnboundTarget)?;
        let (named, passes) = self.material_and_passes(material)?;
        let passes = match usize::try_from(pass) {
            Ok(one) if one < passes => one..one + 1,
            Err(_) if passes > 0 => 0..passes,
            _ => return Err(Skip::NoSuchPass),
        };
        let (values, _) = self.current_view()?;
        if vertices == 0 || instances == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![Record::Procedural(Arc::new(ProceduralDraw {
            target,
            values,
            per_draw: draw_layer(matrix),
            front_face: IMAGE_FRONT_FACE,
            material: named,
            passes: passes.map(PassSelector::Index).collect(),
            topology: match topology {
                DrawTopology::TriangleList => wgpu::PrimitiveTopology::TriangleList,
                DrawTopology::LineList => wgpu::PrimitiveTopology::LineList,
            },
            vertices,
            instances,
        }))])
    }
}
