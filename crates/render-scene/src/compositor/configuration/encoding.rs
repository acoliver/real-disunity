//! Stock-only operations, without allocation-changing access to the renderer.
use super::{ConfiguredFrameRenderer, FrameRenderer};
use worldrunner_render_post::{AttachmentView, FrameError, PostError};

/// Restricted stock composition. Custom features need a separately inventoried builder.
///
/// ```compile_fail
/// # use worldrunner_render_scene::compositor::FrameEncoding;
/// fn custom(mut frame: FrameEncoding<'_>, device: &wgpu::Device,
///           encoder: &mut wgpu::CommandEncoder,
///           features: &mut [worldrunner_render_post::BorrowedFeature<'_>]) {
///     frame.encode(device, encoder, &[], &[], features);
/// }
/// ```
pub struct FrameEncoding<'a>(&'a mut FrameRenderer);
impl FrameRenderer {
    pub fn encoding(&mut self) -> FrameEncoding<'_> {
        FrameEncoding(self)
    }
}
impl ConfiguredFrameRenderer {
    pub fn set_clear_color(&mut self, color: wgpu::Color) -> Result<(), PostError> {
        self.state.current_mut().set_clear_color(color)?;
        self.settings.clear_color = color;
        Ok(())
    }
    pub fn encoding(&mut self) -> FrameEncoding<'_> {
        FrameEncoding(self.state.current_mut())
    }
    /// Retains submitted, host-budgeted environment map views without allocating storage.
    pub fn set_water_environment(
        &mut self,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        self.state.current_mut().set_water_environment(maps)
    }
    pub fn water_receivers(
        &self,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView)> {
        self.state.current().water_receivers()
    }
    pub fn targets(&self) -> &worldrunner_render_post::FrameTargets {
        self.state.current().targets()
    }
    pub fn set_frame_time(&mut self, seconds: f32) -> Result<(), FrameError> {
        self.state.current_mut().set_frame_time(seconds)
    }
    pub fn set_frame_delta(&mut self, delta: f32) -> Result<(), PostError> {
        self.state.current_mut().set_frame_delta(delta)
    }
    pub fn reset_exposure(&self) {
        self.state.current().reset_exposure();
    }
    pub fn encode_output(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) -> Result<(), PostError> {
        self.state.current().encode_output(encoder, output)
    }
    pub fn begin_timing(
        &self,
        frame_id: u64,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        self.state.current().begin_timing(frame_id)
    }
    pub fn take_timings(
        &self,
    ) -> Result<Vec<worldrunner_render_post::FrameTiming>, worldrunner_render_post::TimingError>
    {
        self.state.current().take_timings()
    }
    pub fn unavailable_timing_passes(&self) -> u64 {
        self.state.current().unavailable_timing_passes()
    }
    pub fn dropped_timing_frames(&self) -> u64 {
        self.state.current().dropped_timing_frames()
    }
}
impl FrameEncoding<'_> {
    pub fn encode_scene(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        attachments: (&AttachmentView, &AttachmentView),
        frame: crate::frame::FrameUniform,
    ) -> Result<(), FrameError> {
        super::super::encode_scene(self.0, device, encoder, scene, attachments, frame)
    }
    pub fn encode_shadowed(
        &mut self,
        context: &crate::device::GpuContext,
        view: &crate::SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scene: &mut crate::shadows::ShadowedScene,
    ) -> Result<(), crate::shadows::ShadowError> {
        scene.encode_with(context, view, attachments, encoder, Some(self.0))
    }
}
