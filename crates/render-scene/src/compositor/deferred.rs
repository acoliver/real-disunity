//! The stock scene features' part in the G-buffer observation's lighting pass.
use super::FrameRenderer;
use worldrunner_render_post::{FeatureDeclaration, FrameError, PassSlot, ResourceRef};

use crate::gbuffer::lighting::LightingInputs;

impl FrameRenderer {
    /// With a G-buffer path, declares the lighting pass on a stock scene feature and builds its
    /// inputs for `frame`; without one, nothing.
    ///
    /// `pipelines` are the forward pipelines of the feature's Opaque draws, and `depth` the
    /// scene depth as Opaque declares it. The lighting pass runs before Opaque, so it lights
    /// only when the depth prepass completed that depth (`complete`: Opaque loads it), which is
    /// also when the producers draw. Without it the observation lights nothing and its targets
    /// stay clear, and rendering from the G-buffer is refused: no producer would store the
    /// surfaces Opaque no longer draws. The lighting program's outputs must be the targets it
    /// writes: both carry the reflection products exactly when reflections are installed.
    pub(crate) fn prepare_deferred_lighting(
        &self,
        device: &wgpu::Device,
        (gpu, pipelines): (&crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        (depth, texture, complete): (ResourceRef, &wgpu::Texture, bool),
        frame: &crate::frame::FrameUniform,
        declaration: &mut FeatureDeclaration,
    ) -> Result<Option<LightingInputs>, FrameError> {
        let Some(gbuffer) = &self.gbuffer else {
            return Ok(None);
        };
        let produced = self
            .ssao
            .as_ref()
            .is_some_and(|screen| screen.geometry.gbuffer.is_some());
        if !(produced && (complete || gbuffer.renders())) {
            if gbuffer.renders() {
                return Err(FrameError::FeaturePreparation {
                    feature: "opaque G-buffer",
                    reason: "rendering from the G-buffer needs the depth prepass".to_owned(),
                });
            }
            return Ok(None);
        }
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
