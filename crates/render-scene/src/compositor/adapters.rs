//! Encoder-only adapter for hosts that already uploaded their frame uniforms.
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, DrawContext, FrameError, PassDeclaration, PassSlot,
    PrepareContext, PreparedFeature,
};

use super::FrameRenderer;
use crate::{draw::SceneGpu, renderer::ScenePipelines};

struct Resident<'a> {
    publications: crate::draw::DrawPublications,
    scene: &'a SceneGpu,
    pipelines: &'a ScenePipelines,
}
impl PreparedFeature for Resident<'_> {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn finish_draws(&mut self, context: &mut PrepareContext<'_>) {
        self.publications.publish(context);
    }
    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.publications
            .record(self.scene.draw_color(pass, self.pipelines, None));
    }
}

/// Appends a legacy resident draw to the shared executor without submitting.
/// The caller has already set the frame and flushed instance edits. For staged
/// frame uploads, snapshots and screen effects use [`super::encode_scene`] instead.
/// This entry retains the legacy exposed sRGB output, with no new post look.
pub fn encode_resident(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&SceneGpu, &ScenePipelines),
    attachments: (&AttachmentView, &AttachmentView),
) -> Result<(), FrameError> {
    let (scene, pipelines) = scene;
    let (color, depth) = attachments;
    if scene.has_terrain_pages() {
        scene.prepare_terrain_pages(device, encoder, scene.resident_frame(), color.size())?;
    }
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(renderer.color_attachment(color.resource(), color));
    pass.depth = Some(super::attachment(depth.resource(), depth, true));
    let mut feature = Resident {
        scene,
        pipelines,
        publications: crate::draw::DrawPublications::default(),
    };
    renderer.encode(
        device,
        encoder,
        &[pass],
        &[color.clone(), depth.clone()],
        &mut [BorrowedFeature {
            declaration: super::SceneFeature::declaration(pipelines),
            prepared: &mut feature,
        }],
    )
}
