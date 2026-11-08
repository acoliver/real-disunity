use super::offscreen::encode_player_work;
use super::{FrameRenderer, attachment, attachment_resource};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameError, PassDeclaration, PassSlot,
    PreparedFeature,
};

pub(crate) struct SceneFeature<'a> {
    pub(crate) publications: crate::draw::DrawPublications,
    pub gpu: &'a mut crate::draw::SceneGpu,
    pub pipelines: &'a crate::renderer::ScenePipelines,
    pub frame: crate::frame::FrameUniform,
    pub screen: Option<crate::scene_metadata::Geometry>,
    /// Admitted player program draws, recorded after the stock opaque draw. Present only
    /// without screen-space passes, so the early returns below never skip them.
    pub programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
}
impl SceneFeature<'_> {
    pub fn declaration(pipelines: &crate::renderer::ScenePipelines) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].color_format = Some(pipelines.color_format);
        declaration.passes[0].samples = pipelines.samples;
        declaration.passes[0].depth_format = Some(pipelines.depth.format());
        declaration
    }
}
impl PreparedFeature for SceneFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.gpu.reset_pass_statistics();
        self.publications.install(
            context,
            crate::draw::PassInstallation::Absent,
            self.screen.is_some(),
            self.gpu.ssao,
        );
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
        self.gpu.prepare_view_compaction(context, None, &self.frame);
    }
    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.publications.publish(context, self.gpu);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if context.slot == PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        if let Some(screen) = &self.screen {
            if context.slot == PassSlot::DepthNormalPrepass {
                self.publications.prepass();
                self.publications.record(self.gpu.draw_prepass(
                    pass,
                    screen.prepass_pipelines(),
                    &screen.group,
                ));
                return;
            }
            if let Some((pipelines, group)) = &screen.forward {
                pass.set_bind_group(3, group, &[]);
                self.publications
                    .record_color(self.gpu, self.gpu.draw_color(pass, pipelines, None));
                return;
            }
            pass.set_bind_group(3, &screen.ambient, &[]);
        }
        self.publications
            .record_color(self.gpu, self.gpu.draw_color(pass, self.pipelines, None));
        if let Some(programs) = self
            .programs
            .as_ref()
            .filter(|_| context.slot == PassSlot::Opaque)
        {
            programs.record(pass);
        }
    }
}

/// Adds the velocity attachment, which the main pass clears to zero and never resolves.
pub(crate) fn declare_velocity(
    opaque: &mut PassDeclaration,
    views: &mut Vec<AttachmentView>,
    velocity: Option<&AttachmentView>,
) -> Result<(), FrameError> {
    if let Some(velocity) = velocity {
        let id = attachment_resource("scene velocity", velocity);
        opaque
            .additional_colors
            .push(attachment(id, velocity, false));
        views.push(velocity.with_resource(id)?);
    }
    Ok(())
}

/// Checks the screen-space, water and post settings the scene was uploaded with against the
/// renderer.
fn validate_effects(
    renderer: &mut FrameRenderer,
    gpu: &crate::draw::SceneGpu,
    pipelines: &crate::renderer::ScenePipelines,
) -> Result<(), FrameError> {
    renderer.validate_ssao(pipelines, gpu.ssao.as_ref())?;
    renderer.validate_water(&gpu.water)?;
    renderer.validate_ssr(gpu.ssr.as_ref())?;
    renderer.validate_post(gpu.post)
}

/// Encodes a resident unshadowed scene through the feature boundary.
/// Checked attachments retain their selected mip, array layer and view format.
/// The submission and abandonment rules of [`FrameRenderer`] apply.
pub fn encode_scene(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
    attachments: (&AttachmentView, &AttachmentView),
    frame: crate::frame::FrameUniform,
) -> Result<(), FrameError> {
    let (gpu, pipelines) = scene;
    renderer.validate_taa_depth(attachments.1)?;
    validate_effects(renderer, gpu, pipelines)?;
    if let Some(screen) = &mut renderer.ssao {
        screen.receiver = None;
    }
    renderer.prepare_scene_inputs(device, gpu);
    let depth_mode = renderer.forward_depth(gpu, attachments.1);
    let selected = renderer.color_pipelines(pipelines, depth_mode);
    let pipelines = &selected;
    let writes_velocity = renderer.draws_velocity(pipelines);
    gpu.track_previous_transforms(writes_velocity);
    let velocity = renderer.velocity_attachment(writes_velocity)?;
    let programs = encode_player_work(gpu, encoder, renderer, (pipelines, velocity.is_some()))?;
    gpu.prepare_dynamic(encoder, pipelines)?;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    gpu.prepare_terrain_geometry(device, encoder, &frame, color.size())?;
    gpu.prepare_sky(device, encoder, &frame)?;
    gpu.prepare_terrain_pages(device, encoder, &frame, color.size())?;
    let color_id = attachment_resource("scene color", color);
    let depth_id = renderer.scene_depth_resource(depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(renderer.color_attachment(color_id, color));
    opaque.depth = Some(attachment(depth_id, depth, true));
    if depth_mode == crate::renderer::depth::DepthMode::Reuse {
        opaque.depth.as_mut().expect("scene depth").load =
            worldrunner_render_post::AttachmentLoad::Load;
    }
    let mut declaration = SceneFeature::declaration(pipelines);
    if let Some(screen) = &renderer.ssao {
        crate::ssao::passes::geometry_declaration(&mut declaration, screen.depth.format());
    }
    let mut views = vec![
        color.with_resource(color_id)?,
        depth.with_resource(depth_id)?,
    ];
    declare_velocity(&mut opaque, &mut views, velocity.as_ref())?;
    let mut passes = vec![opaque];
    gpu.plan_celestial(
        device,
        color.size(),
        &mut passes,
        &mut views,
        &mut declaration,
    );
    let mut feature = SceneFeature {
        publications: crate::draw::DrawPublications::default(),
        screen: renderer.geometry_for_depth(depth_mode),
        programs,
        gpu,
        pipelines,
        frame,
    };
    renderer.encode_frame(
        device,
        encoder,
        (&passes, &views),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut feature,
        }],
        (frame, None),
    )
}
