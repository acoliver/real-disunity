//! The TAA block is built on the CPU from the camera data and follows the reset rules.
use super::uniform;
use crate::compositor::FrameRenderer;
use crate::{
    device::GpuContext,
    frame::{FrameUniform, temporal::TemporalView},
    view::{CameraPose, SceneView, Viewport},
};
use glam::{DMat4, DVec4, Mat4, Vec3};
use worldrunner_render_post::{
    AntiAliasing, MsaaSamples, PostResolve, ProjectionJitter, TAA_UNIFORM_BYTES, ToneMap,
};

const SIZE: [u32; 2] = [64, 32];
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn projection() -> Mat4 {
    glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0)
}

fn view_of(current: Mat4, previous: Mat4, valid: bool) -> TemporalView {
    TemporalView {
        unjittered_view_projection: current,
        previous_view_projection: previous,
        jitter_pixels: [0.25, -0.5],
        previous_jitter_pixels: [0.0, 0.0],
        history_valid: valid,
    }
}

/// Where the block sends the current-frame pixel of `world`, as a previous-frame uv.
fn previous_uv(
    block: &worldrunner_render_post::TaaUniform,
    current: Mat4,
    world: Vec3,
) -> [f64; 2] {
    let clip = current * world.extend(1.0);
    let ndc = DVec4::new(
        f64::from(clip.x / clip.w),
        f64::from(clip.y / clip.w),
        f64::from(clip.z / clip.w),
        1.0,
    );
    let flat: Vec<f64> = block.reprojection.iter().flatten().copied().collect();
    let previous = DMat4::from_cols_slice(&flat) * ndc;
    [
        0.5 + 0.5 * previous.x / previous.w,
        0.5 - 0.5 * previous.y / previous.w,
    ]
}

fn assert_close(actual: [f64; 2], expected: [f64; 2]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-5, "{actual:?} vs {expected:?}");
    }
}

#[test]
fn an_identity_camera_reprojects_every_pixel_onto_itself() {
    let camera = projection() * Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0));
    let block = uniform(&view_of(camera, camera, true));
    for world in [Vec3::new(0.0, 0.0, -4.0), Vec3::new(1.0, -0.5, -3.0)] {
        let clip = camera * world.extend(1.0);
        let own = [
            f64::from(0.5 + 0.5 * clip.x / clip.w),
            f64::from(0.5 - 0.5 * clip.y / clip.w),
        ];
        assert_close(previous_uv(&block, camera, world), own);
    }
    assert!(block.history_valid);
    assert_eq!(block.jitter_pixels, [0.25, -0.5]);
}

#[test]
fn a_camera_translation_moves_a_known_point_to_its_previous_uv() {
    let current = projection();
    // The previous camera stood one metre to the right: the point (0, 0, -4) sat at ndc x -0.25.
    let previous = projection() * Mat4::from_translation(Vec3::new(-1.0, 0.0, 0.0));
    let block = uniform(&view_of(current, previous, true));
    assert_close(
        previous_uv(&block, current, Vec3::new(0.0, 0.0, -4.0)),
        [0.375, 0.5],
    );
}

#[test]
fn a_camera_rotation_moves_a_known_point_to_its_previous_uv() {
    let current = projection();
    // The previous camera had yawed so the point straight ahead sat at ndc x = tan(yaw) = 0.5.
    let yaw = 0.5_f32.atan();
    let previous = projection() * Mat4::from_rotation_y(-yaw);
    let block = uniform(&view_of(current, previous, true));
    assert_close(
        previous_uv(&block, current, Vec3::new(0.0, 0.0, -4.0)),
        [0.75, 0.5],
    );
}

#[test]
fn a_frame_without_valid_history_marks_the_block_invalid() {
    let camera = projection();
    let block = uniform(&view_of(camera, camera, false));
    assert!(!block.history_valid);
    assert_eq!(block.to_bytes()[72..76], 0.0_f32.to_le_bytes());
}

#[test]
fn a_singular_camera_gives_an_identity_block_without_history() {
    let block = uniform(&view_of(Mat4::ZERO, projection(), true));
    assert!(!block.history_valid);
    assert_eq!(block.reprojection, DMat4::IDENTITY.to_cols_array_2d());
}

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}

fn taa_renderer(context: &GpuContext) -> FrameRenderer {
    FrameRenderer::hdr_antialiased(
        context,
        SIZE,
        OUTPUT,
        AntiAliasing::new(MsaaSamples::X1, PostResolve::taa(0.9)),
        ToneMap::Reinhard,
        None,
    )
    .unwrap()
}

fn frame(eye_x: i64) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        worldrunner_render_plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

/// Encodes a frame with no passes, as the first half of a frame; the caller decides its fate.
fn begin(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    eye_x: i64,
) -> worldrunner_render_gpu::submission::PreparedFrame {
    let prepared = renderer.prepare_uniform(frame(eye_x)).unwrap();
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_frame(
                context.device(),
                encoder,
                (&[], &[]),
                &mut [],
                (prepared, None),
            )
        })
        .unwrap()
}

fn submit(context: &GpuContext, renderer: &mut FrameRenderer, eye_x: i64) {
    begin(context, renderer, eye_x).submit().unwrap();
    context.device().poll(wgpu::PollType::Poll).unwrap();
}

fn valid(renderer: &FrameRenderer) -> bool {
    let block = renderer
        .taa_block()
        .expect("the frame carries temporal data");
    let lane: [u8; 4] = block[72..76].try_into().unwrap();
    f32::from_le_bytes(lane) == 1.0
}

#[test]
fn a_taa_renderer_builds_with_jitter_selected_automatically() {
    let context = context();
    let renderer = taa_renderer(&context);
    assert_eq!(
        renderer.projection_jitter(),
        Some(ProjectionJitter::default())
    );
}

#[test]
fn the_first_frame_uploads_a_block_without_history_and_the_next_one_with() {
    let context = context();
    let mut renderer = taa_renderer(&context);
    assert_eq!(renderer.taa_block(), None);
    submit(&context, &mut renderer, 0);
    assert!(!valid(&renderer));
    submit(&context, &mut renderer, 250);
    assert!(valid(&renderer));
}

#[test]
fn history_is_invalid_after_a_reset_a_resize_a_jitter_change_and_a_cancelled_first_frame() {
    let context = context();
    let mut renderer = taa_renderer(&context);
    let warm = |renderer: &mut FrameRenderer| {
        submit(&context, renderer, 0);
        submit(&context, renderer, 100);
        begin(&context, renderer, 200).cancel();
        assert!(
            valid(renderer),
            "a cancelled later frame keeps the kept frame as history"
        );
    };
    warm(&mut renderer);
    renderer.reset_temporal_history();
    submit(&context, &mut renderer, 0);
    assert!(!valid(&renderer), "reset");
    warm(&mut renderer);
    assert!(
        renderer
            .resize(context.device(), [SIZE[0] / 2, SIZE[1]])
            .unwrap()
    );
    submit(&context, &mut renderer, 0);
    assert!(!valid(&renderer), "resize");
    warm(&mut renderer);
    renderer
        .set_projection_jitter(Some(ProjectionJitter::new(4).unwrap()))
        .unwrap();
    submit(&context, &mut renderer, 0);
    assert!(!valid(&renderer), "jitter change");
    renderer.reset_temporal_history();
    begin(&context, &mut renderer, 0).cancel();
    submit(&context, &mut renderer, 0);
    assert!(!valid(&renderer), "cancelled first frame");
}

#[test]
fn a_frame_without_temporal_data_clears_the_pending_block() {
    let context = context();
    let mut renderer = taa_renderer(&context);
    submit(&context, &mut renderer, 0);
    assert!(renderer.taa_block().is_some());
    let plain = frame(0);
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_frame(
                context.device(),
                encoder,
                (&[], &[]),
                &mut [],
                (plain, None),
            )
        })
        .unwrap()
        .cancel();
    assert_eq!(renderer.taa_block(), None);
}

#[test]
fn encode_frame_without_encode_output_leaves_the_last_block_pending() {
    // The block belongs to the last encode_frame and is uploaded by the next encode_output;
    // a host that never calls encode_output uploads nothing and the next frame overwrites it.
    let context = context();
    let mut renderer = taa_renderer(&context);
    submit(&context, &mut renderer, 0);
    let first = renderer.taa_block();
    submit(&context, &mut renderer, 500);
    assert_ne!(renderer.taa_block(), first);
    assert_eq!(
        renderer.taa_block().map(|block| block.len()),
        Some(TAA_UNIFORM_BYTES)
    );
}

#[test]
fn a_renderer_without_taa_keeps_no_block_and_uploads_nothing() {
    let context = context();
    let mut renderer =
        FrameRenderer::hdr(&context, SIZE, OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
    submit(&context, &mut renderer, 0);
    assert_eq!(renderer.taa_block(), None);
}
