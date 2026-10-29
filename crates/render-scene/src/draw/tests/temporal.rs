//! Whole-frame checks that projection jitter is opt-in and reaches the scene pass.
use super::decals::{capture_with_renderer, source};
use super::*;
use crate::compositor::FrameRenderer;
use worldrunner_render_post::{FrameConfig, ProjectionJitter};

fn renderer(context: &GpuContext) -> FrameRenderer {
    FrameRenderer::with_output(
        context.device(),
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        COLOR_FORMAT,
    )
    .unwrap()
}

#[test]
fn jitter_explicitly_off_renders_the_bytes_of_a_renderer_without_the_setting() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut absent = renderer(&context);
    let mut toggled = renderer(&context);
    toggled
        .set_projection_jitter(Some(ProjectionJitter::default()))
        .unwrap();
    toggled.set_projection_jitter(None).unwrap();
    for _ in 0..3 {
        let expected = capture_with_renderer(&context, &mut absent, (&mut gpu, &pipelines), true);
        let actual = capture_with_renderer(&context, &mut toggled, (&mut gpu, &pipelines), true);
        assert_eq!(actual, expected);
    }
    assert!(!toggled.has_temporal_history());
}

#[test]
fn jitter_on_moves_the_drawn_scene_and_publishes_history_when_the_frame_is_submitted() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut plain = renderer(&context);
    let unjittered = capture_with_renderer(&context, &mut plain, (&mut gpu, &pipelines), true);
    let mut jittered = renderer(&context);
    jittered
        .set_projection_jitter(Some(ProjectionJitter::default()))
        .unwrap();
    assert!(!jittered.has_temporal_history());
    let moved = capture_with_renderer(&context, &mut jittered, (&mut gpu, &pipelines), true);
    assert_ne!(moved, unjittered);
    assert!(jittered.has_temporal_history());
}

fn previous_frame_block(context: &GpuContext, gpu: &SceneGpu) -> Vec<u8> {
    let device = context.device();
    let size = crate::frame::temporal::PREVIOUS_FRAME_BYTES as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("previous frame readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(gpu.decals.previous_buffer(), 0, &readback, 0, size);
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &readback)
}

fn frame_with_history() -> FrameUniform {
    let mut frame = frame_from(pose());
    frame.animation_time_milliseconds = 2_000;
    frame.temporal = Some(crate::frame::temporal::TemporalView {
        unjittered_view_projection: frame.view_projection,
        previous_view_projection: glam::Mat4::from_translation(glam::Vec3::new(0.25, 0.5, 0.75)),
        jitter_pixels: [0.25, -0.25],
        previous_jitter_pixels: [0.0, 0.0],
        history_valid: true,
        previous_animation_time_ms: 1_984,
        previous_camera_position: glam::Vec3::new(1.0, -4.0, 2.5),
    });
    frame
}

#[test]
fn the_scene_frame_group_carries_the_previous_frame_the_temporal_view_published() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let frame = frame_with_history();
    gpu.set_frame(context.queue(), frame);
    assert_eq!(
        previous_frame_block(&context, &gpu),
        frame.previous_frame_bytes()
    );
}

#[test]
fn a_frame_without_history_hands_shaders_its_own_matrix_camera_and_clock() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, 1).unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut frame = frame_from(pose());
    frame.animation_time_milliseconds = 777;
    gpu.set_frame(context.queue(), frame);
    let block = previous_frame_block(&context, &gpu);
    assert_eq!(block, frame.previous_frame_bytes());
    assert_eq!(&block[76..80], &777_u32.to_le_bytes());
    assert_eq!(&block[..64], &frame.to_bytes()[..64]);
}
