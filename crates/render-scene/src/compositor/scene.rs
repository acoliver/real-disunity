use super::{FrameRenderer, attachment, attachment_resource};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameError, PassDeclaration, PassSlot,
    PreparedFeature,
};

pub(crate) struct SceneFeature<'a> {
    publications: crate::draw::DrawPublications,
    pub gpu: &'a mut crate::draw::SceneGpu,
    pub pipelines: &'a crate::renderer::ScenePipelines,
    pub frame: crate::frame::FrameUniform,
    pub screen: Option<crate::ssao::Geometry>,
}
impl SceneFeature<'_> {
    pub fn declaration(pipelines: &crate::renderer::ScenePipelines) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].color_format = Some(pipelines.color_format);
        declaration.passes[0].samples = pipelines.samples;
        declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
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
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
    }
    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.publications.publish(context);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if let Some(screen) = &self.screen {
            if context.slot == PassSlot::ReflectionMaterial {
                self.publications.record(self.gpu.draw_prepass(
                    pass,
                    screen.reflection.as_ref().expect("reflection metadata"),
                    &screen.group,
                ));
                return;
            }
            if context.slot == PassSlot::DepthNormalPrepass {
                self.publications.record(self.gpu.draw_prepass(
                    pass,
                    &screen.pipelines,
                    &screen.group,
                ));
                return;
            }
            pass.set_bind_group(3, &screen.ambient, &[]);
        }
        self.publications
            .record(self.gpu.draw_color(pass, self.pipelines, None));
    }
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
    renderer.validate_ssao(pipelines, gpu.ssao.as_ref())?;
    renderer.validate_water(gpu.water.as_ref())?;
    renderer.validate_ssr(gpu.ssr.as_ref())?;
    renderer.validate_post(gpu.post)?;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    let color_id = attachment_resource("scene color", color);
    let depth_id = attachment_resource("scene depth", depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(renderer.color_attachment(color_id, color));
    opaque.depth = Some(attachment(depth_id, depth, true));
    let mut declaration = SceneFeature::declaration(pipelines);
    if renderer.ssao.is_some() {
        crate::ssao::passes::geometry_declaration(&mut declaration);
        if renderer.ssr.is_some() {
            crate::ssr::geometry_declaration(&mut declaration);
        }
    }
    let mut feature = SceneFeature {
        publications: crate::draw::DrawPublications::default(),
        screen: renderer.ssao.as_ref().map(|ssao| ssao.geometry.clone()),
        gpu,
        pipelines,
        frame,
    };
    renderer.encode_frame(
        device,
        encoder,
        (
            &[opaque],
            &[
                color.with_resource(color_id)?,
                depth.with_resource(depth_id)?,
            ],
        ),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut feature,
        }],
        (frame, None),
    )
}
