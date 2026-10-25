use super::*;

#[test]
fn gpu_capture_adapters_and_copy_output_match_direct_capture_bytes() {
    let context = required_context();
    let scene = fixture::scene(false);
    let view = fixture::view(&scene, viewport(65, 47));
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    gpu.set_frame(
        context.queue(),
        FrameUniform::new(&view, scene.effects()).unwrap(),
    );
    let expected = direct_capture(&context, &gpu, &pipelines, view.viewport());
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), expected);
    assert_eq!(capture_scene(&context, &scene, &view).unwrap(), expected);
    assert_eq!(
        composed_capture(&context, &gpu, &pipelines, view.viewport()),
        expected
    );
}
fn direct_capture(
    context: &GpuContext,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    viewport: Viewport,
) -> RgbaImage {
    let mut target = OffscreenTarget::new(context.device(), viewport).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.color_view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: target.depth_view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        scene.draw(&mut pass, pipelines);
    }
    target.encode_readback(&mut encoder).unwrap();
    let submitted = encoder.finish().submit().unwrap();
    target.read_encoded(context, submitted).unwrap()
}
fn composed_capture(
    context: &GpuContext,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
    viewport: Viewport,
) -> RgbaImage {
    use worldrunner_render_post::{AttachmentView, ResourceRef};
    let mut renderer = crate::compositor::FrameRenderer::with_output(
        context.device(),
        FrameConfig::new(
            [viewport.width, viewport.height],
            OffscreenTarget::COLOR_FORMAT,
            1,
        ),
        OffscreenTarget::COLOR_FORMAT,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut target = OffscreenTarget::new(context.device(), viewport).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    crate::compositor::encode_resident(
        &mut renderer,
        context.device(),
        &mut encoder,
        (scene, pipelines),
        (&color, &depth),
    )
    .unwrap();
    renderer
        .encode_output(&mut encoder, target.color_view())
        .unwrap();
    target.encode_readback(&mut encoder).unwrap();
    let submitted = encoder.finish().submit().unwrap();
    target.read_encoded(context, submitted).unwrap()
}
