//! Behavioural checks of projection jitter and previous-frame state on the compositor.
use super::FrameRenderer;
use crate::{
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::GpuContext,
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use glam::Mat4;
use worldrunner_render_gpu::configuration::AllocationBudget;
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

fn timed_frame(eye_x: i64, milliseconds: u32) -> FrameUniform {
    let mut frame = frame(eye_x);
    frame.animation_time_milliseconds = milliseconds;
    frame
}

/// Prepares a frame, stages its history and submits the transaction.
fn submit(context: &GpuContext, renderer: &FrameRenderer, eye_x: i64) -> FrameUniform {
    submit_frame(context, renderer, &frame(eye_x))
}

fn submit_frame(
    context: &GpuContext,
    renderer: &FrameRenderer,
    input: &FrameUniform,
) -> FrameUniform {
    let prepared = renderer.prepare_uniform(*input).unwrap();
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
    toggled.set_projection_jitter(Some(jitter())).unwrap();
    toggled.set_projection_jitter(None).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let once = renderer.prepare_uniform(frame(0)).unwrap();
    let twice = renderer.prepare_uniform(once).unwrap();
    assert_eq!(twice.to_bytes(), once.to_bytes());
    assert_eq!(twice.temporal, once.temporal);
}

#[test]
fn a_submitted_frame_becomes_the_previous_frame() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(short)).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit(&context, &renderer, 0);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    assert!(renderer.has_temporal_history());
    renderer
        .set_projection_jitter(Some(ProjectionJitter::new(4).unwrap()))
        .unwrap();
    assert!(!renderer.has_temporal_history());
    submit(&context, &renderer, 0);
    assert!(renderer.has_temporal_history());
    renderer.set_projection_jitter(None).unwrap();
    assert!(!renderer.has_temporal_history());
    assert_eq!(renderer.prepare_uniform(frame(0)).unwrap().temporal, None);
}

#[test]
fn a_frame_without_jitter_data_publishes_nothing() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
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

#[test]
fn a_capture_may_stage_the_temporal_history_it_declares() {
    use worldrunner_render_gpu::submission::capture::CaptureWrites;
    use worldrunner_render_post::FrameError;
    let context = context();
    let device = context.device();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let keys = renderer.capture_write_keys();
    let writes = CaptureWrites::new(keys.clone(), keys.clone(), keys).unwrap();
    let prepared = renderer.prepare_uniform(frame(0)).unwrap();
    let prefix = context
        .submission()
        .begin_capture(device, writes, (), |encoder, _, ()| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap();
    let continuation = prefix.submit().unwrap();
    let suffix = continuation
        .resume(
            device,
            |_, _| Ok::<_, FrameError>(()),
            |encoder, _, (), ()| renderer.stage_temporal_history(encoder, &prepared),
        )
        .unwrap();
    suffix
        .submit()
        .unwrap()
        .finish(device, |_, _| Ok::<_, FrameError>(()))
        .unwrap();
    assert!(renderer.has_temporal_history());
}

fn taa_descriptor() -> worldrunner_render_post::AntiAliasing {
    use worldrunner_render_post::{AntiAliasing, MsaaSamples, PostResolve};
    AntiAliasing::new(MsaaSamples::X1, PostResolve::taa(0.9))
}

#[test]
fn taa_builds_at_construction_with_jitter_enabled_automatically() {
    use worldrunner_render_post::{AntiAliasing, MsaaSamples, PostResolve, ToneMap};
    let context = context();
    let build = |antialiasing: AntiAliasing, effects| {
        FrameRenderer::hdr_antialiased(
            &context,
            SIZE,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            antialiasing,
            ToneMap::Reinhard,
            effects,
        )
        .unwrap()
    };
    let automatic = build(taa_descriptor(), None);
    assert_eq!(automatic.projection_jitter(), Some(jitter()));
    let explicit = ProjectionJitter::new(16).unwrap();
    let chosen = build(taa_descriptor().with_jitter(explicit), None);
    assert_eq!(chosen.projection_jitter(), Some(explicit));
    let authored =
        crate::post_taa_tests::effects(worldrunner_render_plan::SceneAntiAlias::taa(900_000), None);
    let plain = AntiAliasing::new(MsaaSamples::X1, PostResolve::None);
    let from_effects = build(plain, Some(&authored));
    assert_eq!(from_effects.projection_jitter(), Some(jitter()));
}

#[test]
fn clearing_jitter_while_taa_is_resident_is_a_typed_error_that_keeps_the_sequence() {
    use worldrunner_render_post::{PostError, PostResolve};
    let context = context();
    let mut renderer = renderer(&context);
    renderer.post_requested.resolve = PostResolve::taa(0.9);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit(&context, &renderer, 0);
    assert_eq!(
        renderer.set_projection_jitter(None),
        Err(PostError::UnsupportedProfile(
            "TAA requires projection jitter"
        ))
    );
    assert_eq!(renderer.projection_jitter(), Some(jitter()));
    assert!(renderer.has_temporal_history());
    let shorter = ProjectionJitter::new(4).unwrap();
    assert_eq!(renderer.set_projection_jitter(Some(shorter)), Ok(()));
    assert_eq!(renderer.projection_jitter(), Some(shorter));
}

#[test]
fn clearing_jitter_is_allowed_without_taa() {
    use worldrunner_render_post::PostResolve;
    let context = context();
    let mut renderer = renderer(&context);
    assert_eq!(renderer.set_projection_jitter(None), Ok(()));
    for resolve in [PostResolve::None, PostResolve::fxaa(0.5)] {
        renderer.post_requested.resolve = resolve;
        assert_eq!(renderer.set_projection_jitter(Some(jitter())), Ok(()));
        assert_eq!(renderer.set_projection_jitter(None), Ok(()), "{resolve:?}");
        assert_eq!(renderer.projection_jitter(), None);
    }
}

#[test]
fn the_first_frame_reports_its_own_clock_and_camera_as_previous() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let input = timed_frame(250, 4_000);
    let first = temporal(&renderer.prepare_uniform(input).unwrap());
    assert_eq!(first.previous_animation_time_ms, 4_000);
    assert_eq!(first.previous_camera_position, input.camera_position);
}

#[test]
fn a_submitted_frame_publishes_its_clock_and_camera_for_the_next_frame() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let published = submit_frame(&context, &renderer, &timed_frame(250, 4_000));
    let next_input = timed_frame(-900, 4_016);
    let next = temporal(&renderer.prepare_uniform(next_input).unwrap());
    assert!(next.history_valid);
    assert_eq!(next.previous_animation_time_ms, 4_000);
    assert_eq!(next.previous_camera_position, published.camera_position);
    assert_ne!(next.previous_camera_position, next_input.camera_position);
}

#[test]
fn a_cancelled_frame_does_not_publish_its_clock_or_camera() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let kept = submit_frame(&context, &renderer, &timed_frame(0, 1_000));
    let prepared = renderer.prepare_uniform(timed_frame(900, 2_000)).unwrap();
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap()
        .cancel();
    let next = temporal(&renderer.prepare_uniform(timed_frame(900, 3_000)).unwrap());
    assert_eq!(next.previous_animation_time_ms, 1_000);
    assert_eq!(next.previous_camera_position, kept.camera_position);
}

#[test]
fn a_reset_discards_the_previous_clock_and_camera() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit_frame(&context, &renderer, &timed_frame(0, 1_000));
    renderer.reset_temporal_history();
    let input = timed_frame(700, 2_000);
    let next = temporal(&renderer.prepare_uniform(input).unwrap());
    assert!(!next.history_valid);
    assert_eq!(next.previous_animation_time_ms, 2_000);
    assert_eq!(next.previous_camera_position, input.camera_position);
}

#[test]
fn a_frame_prepared_before_a_reset_does_not_republish_its_clock() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    let prepared = renderer.prepare_uniform(timed_frame(0, 1_000)).unwrap();
    let transaction = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &prepared)
        })
        .unwrap();
    renderer.reset_temporal_history();
    transaction.submit().unwrap();
    context.device().poll(wgpu::PollType::Poll).unwrap();
    let input = timed_frame(0, 2_000);
    let next = temporal(&renderer.prepare_uniform(input).unwrap());
    assert!(!next.history_valid);
    assert_eq!(next.previous_animation_time_ms, 2_000);
}

#[test]
fn a_clock_that_moves_backwards_discards_history_and_restarts_the_sequence() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit_frame(&context, &renderer, &timed_frame(0, 5_000));
    submit_frame(&context, &renderer, &timed_frame(0, 5_016));
    let input = timed_frame(300, 4_000);
    let rewound = temporal(&renderer.prepare_uniform(input).unwrap());
    assert!(!rewound.history_valid);
    assert_eq!(rewound.previous_animation_time_ms, 4_000);
    assert_eq!(rewound.previous_camera_position, input.camera_position);
    assert_offset(rewound.jitter_pixels, jitter().offset_pixels(0));
    assert!(!renderer.has_temporal_history());
}

#[test]
fn an_equal_clock_keeps_history() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit_frame(&context, &renderer, &timed_frame(0, 5_000));
    let paused = temporal(&renderer.prepare_uniform(timed_frame(100, 5_000)).unwrap());
    assert!(paused.history_valid);
    assert_eq!(paused.previous_animation_time_ms, 5_000);
}

#[test]
fn a_frame_prepared_before_a_rewind_does_not_republish_after_it() {
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(jitter())).unwrap();
    submit_frame(&context, &renderer, &timed_frame(0, 5_000));
    let stale = renderer.prepare_uniform(timed_frame(0, 5_016)).unwrap();
    let transaction = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.stage_temporal_history(encoder, &stale)
        })
        .unwrap();
    let rewound = renderer.prepare_uniform(timed_frame(0, 100)).unwrap();
    assert!(!temporal(&rewound).history_valid);
    transaction.submit().unwrap();
    context.device().poll(wgpu::PollType::Poll).unwrap();
    assert!(!renderer.has_temporal_history());
}

/// A ratio-aware sequence shifts clip space by the offset over the internal raster extent.
///
/// The renderer here rasterises at `SIZE` for a 2x larger output, so `for_extents` yields 32
/// phases. The expected shift of a projected point is `2 * offset.x / SIZE[0]` horizontally and
/// `-2 * offset.y / SIZE[1]` vertically. Both sides go through f32 matrix math with projected
/// coordinates of magnitude at most 1, so each differs by a few ulp (about 1e-7 each); the bound
/// of 1e-5 is far above that and far below the gap between the internal and output extents that
/// the check must tell apart (at least `|offset| / 64` on one axis, about 2e-4 for the smallest).
#[test]
fn a_ratio_aware_sequence_shifts_the_projection_by_the_offset_over_the_internal_extent() {
    const BOUND: f32 = 1e-5;
    let output = SIZE.map(|extent| extent * 2);
    let sequence = ProjectionJitter::for_extents(SIZE, output).unwrap();
    assert_eq!(sequence.length(), 32);
    let context = context();
    let mut renderer = renderer(&context);
    renderer.set_projection_jitter(Some(sequence)).unwrap();
    let points = [
        glam::Vec3::new(0.0, 0.0, 1.7),
        glam::Vec3::new(1.0, 2.0, 2.5),
    ];
    for index in 0..u64::from(sequence.length()) {
        let prepared = submit(&context, &renderer, 0);
        let unjittered = temporal(&prepared).unjittered_view_projection;
        let [x, y] = sequence.offset_pixels(index);
        assert_offset(temporal(&prepared).jitter_pixels, [x, y]);
        let internal = [2.0 * x / 64.0, -2.0 * y / 32.0];
        let at_output = [2.0 * x / 128.0, -2.0 * y / 64.0];
        for point in points {
            let shift = prepared.view_projection.project_point3(point).truncate()
                - unjittered.project_point3(point).truncate();
            assert!((shift.x - internal[0]).abs() < BOUND, "x at {index}");
            assert!((shift.y - internal[1]).abs() < BOUND, "y at {index}");
            let distinct =
                (shift.x - at_output[0]).abs() > BOUND || (shift.y - at_output[1]).abs() > BOUND;
            assert!(
                distinct,
                "the output extent must be distinguishable at {index}"
            );
        }
    }
}

/// A configured renderer whose post settings request reduced temporal reconstruction at 43x32
/// of a 64x48 output, built through the public configuration path.
fn reduced_temporal_renderer(context: &GpuContext) -> ConfiguredFrameRenderer {
    use worldrunner_render_post as post;
    let mut settings =
        RendererConfiguration::legacy([64, 48], crate::offscreen::OffscreenTarget::COLOR_FORMAT);
    settings.frame = post::FrameConfig::new([64, 48], wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent([43, 32]);
    settings.hdr = Some((1.0, post::ToneMap::Reinhard));
    settings.post = post::HdrPostSettings {
        resolve: post::PostResolve::taa(0.9),
        upscale: Some(post::UpscaleSettings {
            mode: post::UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..post::HdrPostSettings::default()
    };
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

#[test]
fn a_jitter_sequence_shorter_than_the_derived_one_is_rejected_and_the_selection_kept() {
    let context = context();
    let mut renderer = reduced_temporal_renderer(&context);
    let derived = ProjectionJitter::for_extents([43, 32], [64, 48]).unwrap();
    assert_eq!(renderer.projection_jitter(), Some(derived));
    let short = ProjectionJitter::new(derived.length() - 1).unwrap();
    let low_level = renderer.renderer_mut();
    assert!(low_level.set_projection_jitter(Some(short)).is_err());
    assert_eq!(low_level.projection_jitter(), Some(derived));
    assert!(low_level.set_projection_jitter(None).is_err());
    assert_eq!(low_level.projection_jitter(), Some(derived));
    let longer = ProjectionJitter::new(derived.length() + 1).unwrap();
    low_level.set_projection_jitter(Some(longer)).unwrap();
    assert_eq!(renderer.projection_jitter(), Some(longer));
}
