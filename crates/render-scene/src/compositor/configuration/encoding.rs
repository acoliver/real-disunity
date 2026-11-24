//! Stock-only operations, without allocation-changing access to the renderer.
use super::{ConfiguredFrameRenderer, FrameRenderer};
use worldrunner_render_post::{AttachmentView, BorrowedFeature, FrameError, PostError};

/// Restricted stock composition. It runs the stock frame and the features the configuration
/// inventoried in [`RendererConfiguration::game_passes`](super::RendererConfiguration::game_passes);
/// no other feature reaches a frame through it.
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
    /// The game's declared targets of this generation; none are declared without an inventory.
    pub(crate) fn game_targets(&self) -> crate::compositor::GameTargets {
        crate::compositor::GameTargets::new(self.game.store.clone().unwrap_or_default())
    }
    pub(crate) fn timing_state(&self) -> worldrunner_render_post::TimingState {
        self.chain.timing_state()
    }
    pub fn encoding(&mut self) -> FrameEncoding<'_> {
        FrameEncoding(self)
    }
}
impl ConfiguredFrameRenderer {
    /// Replays opaque terms with this frame's AO and environment bindings.
    pub fn encode_term_capture(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        capture: &crate::compositor::terms::TermCapture,
        scene: (
            &crate::draw::SceneGpu,
            Option<(&wgpu::BindGroupLayout, &wgpu::BindGroup)>,
        ),
    ) {
        capture.encode_with_frame(
            device,
            encoder,
            scene.0,
            super::super::terms::FrameBindings {
                shadow: scene
                    .1
                    .map(|(layout, group)| super::super::terms::ShadowBinding { layout, group }),
                frame: Some(self.state.current()),
            },
        );
    }
    /// Borrows an existing configured frame product without allocating a target.
    pub fn feature_target(&self, feature: super::super::FeatureTarget) -> Option<&wgpu::Texture> {
        self.state.current().feature_target(feature)
    }

    pub fn feature_capture(
        &self,
        feature: super::super::FeatureTarget,
    ) -> Option<super::super::FeatureCapture<'_>> {
        self.state.current().feature_capture(feature)
    }

    pub fn set_clear_color(&mut self, color: wgpu::Color) -> Result<(), PostError> {
        self.state.current_mut().set_clear_color(color)?;
        self.settings.clear_color = color;
        Ok(())
    }
    pub fn encoding(&mut self) -> FrameEncoding<'_> {
        FrameEncoding(self.state.current_mut())
    }
    /// Restores the stock empty bindings for consumers not supplied by a scene.
    pub fn clear_environment(&mut self, device: &wgpu::Device) {
        self.state.current_mut().clear_environment(device);
        if let Some(settings) = self.settings.ssr.filter(|s| s.enabled) {
            self.state
                .current_mut()
                .set_reflection_composition(crate::ssr::composition(settings.composition));
        }
    }
    /// Retains submitted, host-budgeted environment map views without allocating storage.
    pub fn set_water_environment(
        &mut self,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        self.state.current_mut().set_water_environment(maps)
    }
    pub fn set_reflection_environment(
        &mut self,
        device: &wgpu::Device,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        self.state
            .current_mut()
            .set_reflection_environment(device, maps)
    }
    /// Installs retained scene maps only for the selected consumers.
    pub fn set_scene_environment(
        &mut self,
        device: &wgpu::Device,
        maps: &crate::sky::preparation::PreparedEnvironmentMaps,
        consumers: worldrunner_render_plan::SceneEnvironmentConsumers,
    ) -> Result<(), crate::sky::preparation::PreparationError> {
        use worldrunner_render_plan::SceneEnvironmentConsumers;
        match consumers {
            SceneEnvironmentConsumers::Water => self.set_water_environment(maps),
            SceneEnvironmentConsumers::Reflections => self
                .state
                .current_mut()
                .set_reflection_only_environment(device, maps),
            SceneEnvironmentConsumers::Both => self.set_reflection_environment(device, maps),
        }
    }
    pub fn set_reflection_composition(
        &mut self,
        composition: worldrunner_render_lighting::reflections::Composition,
    ) {
        self.state
            .current_mut()
            .set_reflection_composition(composition);
    }
    pub fn water_receivers(
        &self,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView)> {
        self.state.current().water_receivers()
    }
    pub fn targets(&self) -> &worldrunner_render_post::FrameTargets {
        self.state.current().targets()
    }
    /// The targets the game's factories declared, as the live generation holds them. A
    /// renderer built from a clone of the inventory has targets of its own.
    pub fn game_targets(&self) -> crate::compositor::GameTargets {
        self.state.current().game_targets()
    }
    /// The projection jitter the configuration selected, `None` when the scene passes render
    /// unjittered. Derived from the settings on construction and on every reconfiguration.
    pub fn projection_jitter(&self) -> Option<worldrunner_render_post::ProjectionJitter> {
        self.state.current().projection_jitter()
    }
    #[cfg(test)]
    pub(crate) fn force_material_mip_bias(&self, bias: Option<f32>) {
        self.state.current().force_material_mip_bias(bias);
    }
    /// Discards the previous-frame camera, for camera cuts; the jitter sequence restarts.
    pub fn reset_temporal_history(&self) {
        self.state.current().reset_temporal_history();
    }
    /// Sequence position of the next frame, counting submitted frames since the last reset.
    pub fn projection_jitter_index(&self) -> u64 {
        self.state.current().projection_jitter_index()
    }
    /// Whether the next frame has a previous camera to reproject from.
    pub fn has_temporal_history(&self) -> bool {
        self.state.current().has_temporal_history()
    }
    pub fn set_frame_time(&mut self, time: std::time::Duration) {
        self.state.current_mut().set_frame_time(time);
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
    pub fn encode_resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        self.state.current().encode_resolve(encoder);
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
    /// Encodes a resident unshadowed scene through the feature boundary.
    /// Checked attachments retain their selected mip, array layer and view format.
    /// The submission and abandonment rules of [`FrameRenderer`] apply.
    ///
    /// This is [`prepare_stock_scene`](super::super::prepare_stock_scene) encoded alone: callers
    /// that add passes or features of their own prepare the stock scene themselves and forward
    /// its offscreen operations to [`FrameRenderer::encode_frame`].
    pub fn encode_scene(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        attachments: (&AttachmentView, &AttachmentView),
        frame: crate::frame::FrameUniform,
    ) -> Result<(), FrameError> {
        let mut stock =
            super::super::prepare_stock_scene(self.0, device, encoder, scene, attachments, frame)?;
        let plan = stock.plan();
        let (passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
        let (declaration, frame) = (stock.declaration(), stock.frame());
        let offscreen = stock.take_offscreen_operations();
        self.0.encode_frame(
            device,
            encoder,
            (&passes, &views),
            offscreen
                .as_ref()
                .map(|work| work as &dyn worldrunner_render_post::FrameOperations),
            &mut [BorrowedFeature {
                declaration,
                prepared: &mut stock,
            }],
            &(frame, None),
        )
    }
    /// The pass that shows a game screen named `resource` in the renderer's scene colour format.
    /// Build it once and reuse it for every frame that shows a screen.
    ///
    /// # Errors
    /// Fails when the device cannot build the composite for that format.
    pub fn game_screen_pass(
        &self,
        device: &wgpu::Device,
        resource: worldrunner_render_post::ResourceRef,
    ) -> Result<super::super::GameScreenPass, FrameError> {
        super::super::GameScreenPass::new(&*self.0, device, resource)
    }
    /// Encodes a frame whose scene is the game's screen, in place of
    /// [`Self::encode_scene`]'s stock scene. The scene's admitted offscreen operations run before
    /// the screen is sampled; its UI overlay draws over it in the output stage.
    pub fn encode_game_screen(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &mut crate::draw::SceneGpu,
        frame: super::super::GameScreenFrame<'_>,
    ) -> Result<(), FrameError> {
        super::super::game_screen_frame::encode_game_screen(self.0, device, encoder, scene, frame)
    }
    /// Captures terms before the fitted frame releases transaction-owned scene state.
    pub fn render_depth_fitted(
        &mut self,
        context: &crate::device::GpuContext,
        view: &crate::SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        frame: (Option<u64>, bool, &[crate::compositor::terms::TermCapture]),
        scene: &mut crate::shadows::ShadowedScene,
    ) -> Result<Option<crate::shadows::sdsm::AdoptedFit>, crate::shadows::ShadowError> {
        scene.render_depth_fitted_host(context, view, attachments, frame, self.0)
    }
    pub fn encode_shadowed(
        &mut self,
        context: &crate::device::GpuContext,
        view: &crate::SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scene: &mut crate::shadows::ShadowedScene,
    ) -> Result<(), crate::shadows::ShadowError> {
        scene.encode_with(context, view, attachments, encoder, self.0)
    }
}
