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
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    frame
        .encode_output(
            &mut encoder,
            &out.create_view(&wgpu::TextureViewDescriptor::default()),
        )
        .unwrap();
    context.queue().submit([encoder.finish()]);
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
