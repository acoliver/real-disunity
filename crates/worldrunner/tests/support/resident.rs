use worldrunner::scene::{
    gpu::{compositor, draw::SceneGpu, renderer::ScenePipelines, wgpu},
    post::{AttachmentView, FrameError, ResourceRef},
};

pub fn encode(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&SceneGpu, &ScenePipelines),
    attachments: (&wgpu::TextureView, &wgpu::TextureView),
) -> Result<(), FrameError> {
    let color = AttachmentView::new(
        ResourceRef::new("consumer scene color"),
        attachments.0.texture(),
    )?;
    let depth = AttachmentView::new(
        ResourceRef::new("consumer scene depth"),
        attachments.1.texture(),
    )?;
    compositor::encode_resident(
        &mut compositor::FrameRenderer::new(device),
        device,
        encoder,
        scene,
        (&color, &depth),
    )
}
