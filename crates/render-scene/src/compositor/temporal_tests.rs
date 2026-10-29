//! Behavioural checks of projection jitter and previous-frame state on the compositor.
use super::FrameRenderer;
use crate::{
    device::GpuContext,
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use glam::Mat4;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::ProjectionJitter;

const SIZE: [u32; 2] = [64, 32];

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}

fn renderer(context: &GpuContext) -> FrameRenderer {
    FrameRenderer::with_output(
        context.device(),
        worldrunner_render_post::FrameConfig::new(
            SIZE,
            crate::offscreen::OffscreenTarget::COLOR_FORMAT,
            1,
        ),
        crate::offscreen::OffscreenTarget::COLOR_FORMAT,
    )
    .unwrap()
}

fn view(eye_x: i64) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap()
}

fn frame(eye_x: i64) -> FrameUniform {
    FrameUniform::unlit(&view(eye_x))
}

fn jitter() -> ProjectionJitter {
    ProjectionJitter::default()
}

/// Prepares a frame, stages its history and submits the transaction.
fn submit(context: &GpuContext, renderer: &FrameRenderer, eye_x: i64) -> FrameUniform {
    let prepared = renderer.prepare_uniform(frame(eye_x)).unwrap();
    let transaction = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap();
    transaction.submit().unwrap();
    context.device().poll(wgpu::PollType::Poll).unwrap();
    prepared
}

fn temporal(frame: &FrameUniform) -> crate::frame::temporal::TemporalView {
    frame.temporal.expect("jitter is on")
}

fn assert_offset(actual: [f32; 2], expected: [f32; 2]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
}

fn assert_matrix_close(actual: Mat4, expected: Mat4) {
    let (actual, expected) = (actual.to_cols_array(), expected.to_cols_array());
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() <= 1e-4 * e.abs().max(1.0), "{a} vs {e}");
    }
}

#[test]
fn jitter_is_off_by_default_and_the_prepared_frame_is_untouched() {
    let context = context();
    let renderer = renderer(&context);
    assert_eq!(renderer.projection_jitter(), None);
    let input = frame(0);
    let prepared = renderer.prepare_uniform(input).unwrap();
    assert_eq!(prepared.temporal, None);
    assert_eq!(prepared.to_bytes(), input.to_bytes());
    assert_eq!(prepared.selection_view_projection(), input.view_projection);
}

#[test]
fn explicitly_off_matches_a_renderer_that_never_had_jitter() {
    let context = context();
    let absent = renderer(&context);
    let mut toggled = renderer(&context);
    toggled.set_projection_jitter(Some(jitter()));
    toggled.set_projection_jitter(None);
    for eye_x in [0, 250, -4_000] {
        let expected = submit(&context, &absent, eye_x);
        let actual = submit(&context, &toggled, eye_x);
        assert_eq!(actual.to_bytes(), expected.to_bytes());
        assert_eq!(actual, expected);
    }
    assert!(!toggled.has_temporal_history());
}

#[test]
fn first_frame_is_jittered_and_has_no_history() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let input = frame(0);
    let prepared = renderer.prepare_uniform(input).unwrap();
    let temporal = temporal(&prepared);
    assert!(!temporal.history_valid);
    assert_eq!(temporal.unjittered_view_projection, input.view_projection);
    assert_eq!(temporal.previous_view_projection, input.view_projection);
    let offset = jitter().offset_pixels(0);
    assert_offset(temporal.jitter_pixels, offset);
    assert_offset(temporal.previous_jitter_pixels, offset);
    let expected = view(0)
        .with_raster_sample(
            Viewport {
                width: SIZE[0],
                height: SIZE[1],
            },
            offset,
        )
        .unwrap()
        .view_projection_matrix();
    assert_matrix_close(prepared.view_projection, expected);
    assert_ne!(prepared.view_projection, input.view_projection);
    assert_eq!(prepared.selection_view_projection(), input.view_projection);
}

#[test]
fn jitter_changes_only_the_matrix_lanes_of_the_scene_block() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let input = frame(0);
    let jittered = renderer.prepare_uniform(input).unwrap().to_bytes();
    let plain = input.to_bytes();
    assert_ne!(jittered[..64], plain[..64]);
    assert_eq!(jittered[64..], plain[64..]);
}

#[test]
fn preparing_twice_does_not_jitter_twice() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let once = renderer.prepare_uniform(frame(0)).unwrap();
    let twice = renderer.prepare_uniform(once).unwrap();
    assert_eq!(twice.to_bytes(), once.to_bytes());
    assert_eq!(twice.temporal, once.temporal);
}

#[test]
fn a_submitted_frame_becomes_the_previous_frame() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let first = temporal(&submit(&context, &renderer, 0));
    assert!(renderer.has_temporal_history());
    let second = temporal(&renderer.prepare_uniform(frame(500)).unwrap());
    assert!(second.history_valid);
    assert_eq!(
        second.previous_view_projection,
        first.unjittered_view_projection
    );
    assert_offset(second.previous_jitter_pixels, first.jitter_pixels);
    assert_offset(second.jitter_pixels, jitter().offset_pixels(1));
    assert_ne!(
        second.unjittered_view_projection,
        second.previous_view_projection
    );
}

#[test]
fn the_jitter_sequence_advances_one_step_per_submitted_frame_and_wraps() {
    let context = context();
    let mut renderer = renderer(&context);
    let short = ProjectionJitter::new(3).unwrap();
    renderer.set_projection_jitter(Some(short));
    for index in 0..7_u64 {
        let prepared = submit(&context, &renderer, 0);
        assert_offset(
            temporal(&prepared).jitter_pixels,
            short.offset_pixels(index),
        );
    }
    assert_offset(
        temporal(&renderer.prepare_uniform(frame(0)).unwrap()).jitter_pixels,
        short.offset_pixels(7),
    );
}

#[test]
fn a_cancelled_frame_leaves_no_history_and_does_not_advance_the_sequence() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let kept = submit(&context, &renderer, 0);
    let prepared = renderer.prepare_uniform(frame(900)).unwrap();
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap()
        .cancel();
    let next = temporal(&renderer.prepare_uniform(frame(900)).unwrap());
    assert_eq!(
        next.previous_view_projection,
        temporal(&kept).unjittered_view_projection
    );
    assert_offset(next.jitter_pixels, jitter().offset_pixels(1));
}

#[test]
fn a_cancelled_first_frame_leaves_the_next_frame_without_history() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let prepared = renderer.prepare_uniform(frame(0)).unwrap();
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap()
        .cancel();
    assert!(!renderer.has_temporal_history());
    assert!(!temporal(&renderer.prepare_uniform(frame(0)).unwrap()).history_valid);
}

#[test]
fn a_failed_preparation_leaves_no_history() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let prepared = renderer.prepare_uniform(frame(0)).unwrap();
    let failed = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)?;
            Err::<(), _>(worldrunner_render_post::FrameError::WaterSettings(
                "later stage failed",
            ))
        });
    assert!(failed.is_err());
    assert!(!renderer.has_temporal_history());
    assert!(!temporal(&renderer.prepare_uniform(frame(0)).unwrap()).history_valid);
}

#[test]
fn a_camera_cut_discards_history_and_restarts_the_sequence() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    submit(&context, &renderer, 0);
    submit(&context, &renderer, 100);
    renderer.reset_temporal_history();
    assert!(!renderer.has_temporal_history());
    let next = temporal(&renderer.prepare_uniform(frame(5_000)).unwrap());
    assert!(!next.history_valid);
    assert_eq!(
        next.previous_view_projection,
        next.unjittered_view_projection
    );
    assert_offset(next.jitter_pixels, jitter().offset_pixels(0));
}

#[test]
fn a_frame_prepared_before_a_cut_does_not_republish_after_it() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    let prepared = renderer.prepare_uniform(frame(0)).unwrap();
    let transaction = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap();
    renderer.reset_temporal_history();
    transaction.submit().unwrap();
    context.device().poll(wgpu::PollType::Poll).unwrap();
    assert!(!renderer.has_temporal_history());
}

#[test]
fn a_resize_discards_history() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    submit(&context, &renderer, 0);
    assert!(renderer.resize(context.device(), [32, 32]).unwrap());
    assert!(!renderer.has_temporal_history());
    let next = temporal(&renderer.prepare_uniform(frame(0)).unwrap());
    assert!(!next.history_valid);
    assert_offset(next.jitter_pixels, jitter().offset_pixels(0));
}

#[test]
fn changed_jitter_settings_discard_history_and_identical_ones_keep_it() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    submit(&context, &renderer, 0);
    renderer.set_projection_jitter(Some(jitter()));
    assert!(renderer.has_temporal_history());
    renderer.set_projection_jitter(Some(ProjectionJitter::new(4).unwrap()));
    assert!(!renderer.has_temporal_history());
    submit(&context, &renderer, 0);
    assert!(renderer.has_temporal_history());
    renderer.set_projection_jitter(None);
    assert!(!renderer.has_temporal_history());
    assert_eq!(renderer.prepare_uniform(frame(0)).unwrap().temporal, None);
}

#[test]
fn a_frame_without_jitter_data_publishes_nothing() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter()));
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &frame(0))
        })
        .unwrap()
        .submit()
        .unwrap();
    assert!(!renderer.has_temporal_history());
}

#[test]
fn the_public_descriptor_selects_jitter_and_defaults_to_off() {
    use worldrunner_render_post::{AntiAliasing, MsaaSamples, PostResolve, ToneMap};
    let context = context();
    let build = |antialiasing| {
        FrameRenderer::hdr_antialiased(
            &context,
            SIZE,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            antialiasing,
            ToneMap::Reinhard,
            None,
        )
        .unwrap()
    };
    let plain = AntiAliasing::new(MsaaSamples::X1, PostResolve::None);
    assert_eq!(build(plain).projection_jitter(), None);
    assert_eq!(
        build(plain.with_jitter(jitter())).projection_jitter(),
        Some(jitter())
    );
}
