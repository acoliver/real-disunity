use super::*;

pub(super) fn assert_pending_submission_cleanup(mut renderer: Renderer) {
    let window = Arc::downgrade(&renderer.window);
    let ready = renderer
        .execution
        .acquire(&renderer.context, || Ok(()))
        .unwrap()
        .expect("native surface must supply a frame");
    let mut encoder = renderer
        .context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let output = ready
            .frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    let commands = encoder.finish();
    let mut complete = None;
    let ticket =
        renderer
            .execution
            .submit_with_completion(ready, commands, &renderer.context, |callback| {
                complete = Some(callback)
            });
    // Finish GPU work but withhold the completion acknowledgment through production cleanup.
    ticket.wait(renderer.context.device()).unwrap();
    assert!(!ticket.is_complete());
    let mut renderer = Some(renderer);
    crate::app::cleanup(&mut renderer);
    assert!(renderer.is_none());
    assert!(
        window.upgrade().is_none(),
        "surface must release its window"
    );
    assert!(
        !ticket.is_complete(),
        "cleanup must not dispatch completion"
    );
    complete.take().unwrap()();
    assert!(ticket.is_complete());
}
