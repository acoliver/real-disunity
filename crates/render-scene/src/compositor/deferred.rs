//! The stock scene features' part in the G-buffer observation's lighting pass.
use super::FrameRenderer;
use worldrunner_render_post::{FeatureDeclaration, FrameError, PassSlot, ResourceRef};

use crate::gbuffer::lighting::LightingInputs;

impl FrameRenderer {
    /// With the G-buffer observation, declares the lighting pass on a stock scene feature and
    /// builds its inputs for `frame`; without it, nothing.
    ///
    /// `pipelines` are the forward pipelines of the feature's Opaque draws, and `depth` the
    /// scene depth that Opaque completes, as that pass declares it. The lighting program's
    /// outputs must be the targets the renderer allocated: both carry the reflection products
    /// exactly when reflections are installed.
    pub(crate) fn prepare_deferred_lighting(
        &self,
        device: &wgpu::Device,
        (gpu, pipelines): (&crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        (depth, texture): (ResourceRef, &wgpu::Texture),
        frame: &crate::frame::FrameUniform,
        declaration: &mut FeatureDeclaration,
    ) -> Result<Option<LightingInputs>, FrameError> {
        let Some(gbuffer) = &self.gbuffer else {
            return Ok(None);
        };
        let (format, reflections) = gbuffer.lighting_outputs();
        if gpu
            .deferred_lighting_program(pipelines, frame)
            .reflections()
            != reflections
        {
            return Err(FrameError::AttachmentContract(PassSlot::OpaqueLighting));
        }
        declaration
            .passes
            .push(crate::gbuffer::lighting::feature_pass(
                depth,
                format,
                reflections,
            ));
        gpu.deferred_lighting_inputs(device, pipelines, (gbuffer, texture), frame)
            .map(Some)
    }
}
