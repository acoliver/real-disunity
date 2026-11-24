use worldrunner::scene::{
    gpu::{compositor, draw::SceneGpu, frame::FrameUniform, renderer::ScenePipelines, wgpu},
    post::{AttachmentView, FrameError, ResourceRef},
};

pub fn encode(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&mut SceneGpu, &ScenePipelines),
    attachments: (&wgpu::TextureView, &wgpu::TextureView),
    frame: &FrameUniform,
) -> Result<(), FrameError> {
    let color = AttachmentView::new(
        ResourceRef::new("consumer scene color"),
        attachments.0.texture(),
    )?;
    let depth = AttachmentView::new(
        ResourceRef::new("consumer scene depth"),
        attachments.1.texture(),
    )?;
    compositor::FrameRenderer::new(device)
        .encoding()
        .encode_scene(
            device,
            encoder,
            scene,
            worldrunner_render_scene::compositor::ColorDepth {
                color: &color,
                depth: &depth,
            },
            *frame,
        )
}
