use super::*;

#[test]
fn post_timing_and_reset_follow_output_boundaries_not_slot_selection() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let mut frame = FrameRenderer::hdr(
        &context,
        [4, 4],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        worldrunner_render_post::ToneMap::Reinhard,
        None,
    )
    .unwrap();
    let out = worldrunner_render_post::color_texture(
        device,
        "timing fixture",
        [4, 4],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    frame.reset_exposure();
    frame.select_frame_slot(2);
    frame.set_frame_time(0.25).unwrap();
    frame.set_frame_time(0.5).unwrap();
    assert_eq!(frame.frame_delta.to_bits(), 0.5_f32.to_bits());
    assert!(frame.exposure_reset.get());
    let mut encoder = context.submission().begin(device).unwrap();
    frame
        .encode_output(
            &mut encoder,
            &out.create_view(&wgpu::TextureViewDescriptor::default()),
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    device.poll(wgpu::PollType::Poll).unwrap();
    frame.select_frame_slot(0);
    frame.set_frame_time(0.75).unwrap();
    assert_eq!(frame.frame_delta.to_bits(), 0.25_f32.to_bits());
    assert!(!frame.exposure_reset.get());
    assert_eq!(frame.set_frame_delta(f32::NAN), Err(PostError::FrameDelta));
    assert_eq!(frame.frame_delta.to_bits(), 0.25_f32.to_bits());
    frame.set_frame_time(100.0).unwrap();
    assert_eq!(frame.frame_delta.to_bits(), 10.0_f32.to_bits());
}

#[test]
fn cancelled_and_failed_preparations_release_frame_renderer_timing() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::TIMESTAMP_QUERY,
    ))
    .unwrap();
    let renderer = FrameRenderer::new(context.device());
    renderer
        .configure_timing(
            context.device(),
            context.queue(),
            worldrunner_render_post::TimingMode::Required,
        )
        .unwrap();
    let output = worldrunner_render_post::color_texture(
        context.device(),
        "timing output",
        [1, 1],
        crate::offscreen::OffscreenTarget::COLOR_FORMAT,
    );
    let view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.begin_timing(1).unwrap();
            renderer.encode_output(encoder, &view)
        })
        .unwrap();
    prepared.cancel();
    assert!(renderer.take_timings().unwrap().is_empty());
    let failed = context.submission().prepare(context.device(), |_, _| {
        renderer.begin_timing(2).unwrap();
        Err::<(), _>("failed before finish")
    });
    assert!(failed.is_err());
    assert_timing_unwind(&context, &renderer);
    let submitted = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.begin_timing(4).unwrap();
            renderer.encode_output(encoder, &view)
        })
        .unwrap()
        .submit()
        .unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
    let timings = renderer.take_timings().unwrap();
    assert_eq!(timings.len(), 1);
    assert_eq!(timings[0].frame_id, 4);
    assert!(!timings[0].passes.is_empty());
}

fn assert_timing_unwind(context: &crate::device::GpuContext, renderer: &FrameRenderer) {
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = context
            .submission()
            .prepare(context.device(), |_, _| -> Result<(), ()> {
                renderer.begin_timing(3).unwrap();
                panic!("timing preparation");
            });
    }));
    assert!(unwound.is_err());
}
