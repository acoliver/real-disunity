//! The deferred lighting pass of the G-buffer observation, drawn by the stock scene features
//! with the same forward sibling, frame group and engine sets as their Opaque draws.
use std::rc::Rc;

use worldrunner_render_post::FrameError;

use super::SceneGpu;
use crate::frame::FrameUniform;
use crate::gbuffer::lighting::{LightingInputs, LightingProgram};
use crate::renderer::ScenePipelines;

impl SceneGpu {
    /// The lighting program of the sibling of `pipelines` that colour draws in `frame` select.
    pub(crate) fn deferred_lighting_program(
        &self,
        pipelines: &ScenePipelines,
        frame: &FrameUniform,
    ) -> Rc<LightingProgram> {
        self.selected_pipelines(pipelines, frame)
            .deferred_lighting()
    }

    /// Set 1 of the lighting pass for `frame`: the G-buffer, the completed scene `depth`, this
    /// scene's lighting-record table and the camera block. A table the eight-bit G1.a index
    /// cannot address is refused here, before any pass records.
    pub(crate) fn deferred_lighting_inputs(
        &self,
        device: &wgpu::Device,
        pipelines: &ScenePipelines,
        targets: (&crate::gbuffer::Gbuffer, &wgpu::Texture),
        frame: &FrameUniform,
    ) -> Result<LightingInputs, FrameError> {
        let program = self.deferred_lighting_program(pipelines, frame);
        let records = self.lighting_records.buffer(device)?;
        Ok(LightingInputs::new(
            device, &program, targets, &records, frame,
        ))
    }

    /// Records the lighting pass. The caller binds sets 2 and 3 as for its Opaque draws.
    pub(crate) fn draw_deferred_lighting(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &ScenePipelines,
        inputs: &LightingInputs,
    ) {
        let frame = self.drawing_uniform();
        let program = self.deferred_lighting_program(pipelines, &frame);
        pass.set_bind_group(0, &self.drawing_frame_group(), &[]);
        program.draw(pass, inputs);
    }

    /// The lighting records the material uniforms index.
    #[cfg(test)]
    pub(crate) fn lighting_record_table(&self) -> &crate::gbuffer::records::LightingRecordTable {
        self.lighting_records.table()
    }
}
