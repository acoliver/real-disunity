//! A player's offscreen work handed to the retained host runs in the next frame the viewer
//! renders, on the viewer's device and in that frame's submission.
use super::*;
use crate::RetainedHost;
use std::collections::BTreeMap;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_program::ids::{CameraId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::offscreen::PendingOffscreen;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, OffscreenFrame, Record, RenderDimension, RenderTextureSpec,
};
use worldrunner_render_program::unity::offscreen::{OffscreenState, PendingImage};
use worldrunner_render_program::unity::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::shader::PassSelector;

const TARGET: TextureId = TextureId(10);
const ARENA: u64 = 1 << 20;
const YELLOW: [u8; 4] = [255, 255, 0, 255];

/// One camera that sees nothing and clears an 8 by 8 render texture to yellow.
fn clear() -> OffscreenFrame {
    OffscreenFrame {
        render_textures: vec![RenderTextureSpec {
            id: TARGET,
            size: [8, 8],
            dimension: RenderDimension::D2,
            graphics_format: 8,
            depth_bits: 0,
            mips: 1,
            srgb: false,
            generation: 0,
        }],
        cameras: BTreeMap::from([(
            CameraId(1),
            Camera {
                culling_mask: 0,
                clear_color: Some([1.0, 1.0, 0.0, 1.0]),
                clear_depth: true,
                pass: PassSelector::Index(0),
                values: PropertyLayer::default(),
                front_face: wgpu::FrontFace::Cw,
                renderers: Arc::default(),
            },
        )]),
        records: vec![Record::CameraRender {
            camera: CameraId(1),
            target: TARGET,
        }],
        ..OffscreenFrame::default()
    }
}

fn render_frame(host: &mut RetainedHost, renderer: &mut Renderer) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_mins(1);
    while !renderer.render_retained(host).unwrap() {
        renderer.poll().unwrap();
        assert!(std::time::Instant::now() < deadline, "frame timed out");
        std::thread::yield_now();
    }
}

/// The player's runtime on `device` and `authority` and its arena, as `PlayerGpu::on` makes them.
fn runtime_on(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
) -> (ProgramRuntime, ProgramArena) {
    let limits = device.limits();
    let runtime = ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(ARENA));
    let mut configuration = Configuration::new(
        authority,
        (),
        AllocationSet::default(),
        &limits,
        AllocationBudget(ARENA * 4),
    )
    .unwrap();
    let reservation = configuration
        .reserve::<()>(
            &limits,
            ProgramRuntime::arena_allocations_within(ARENA, &limits),
        )
        .unwrap();
    let arena = runtime.open_arena(reservation).unwrap();
    (runtime, arena)
}

/// The operations of one camera clearing the render texture to yellow on `device` and
/// `authority`, with the read-back of it. The last value keeps the runtime and its arena alive:
/// the operations join a frame only while their arena's owner has not retired.
pub(crate) fn clearing_operations(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
) -> (PendingOffscreen, PendingImage, Box<dyn std::any::Any>) {
    let (mut runtime, arena) = runtime_on(device, authority);
    let mut state = OffscreenState::new(&runtime);
    let mut operations = state.build(&mut runtime, &arena, &clear()).unwrap();
    let image = operations.read_back(TARGET, 0, 0).unwrap();
    (operations.into_pending(), image, Box::new((runtime, arena)))
}

/// The operations of [`clearing_operations`] on a texture stored as sRGB, as a game's screen is,
/// exported as the game's screen, with the texture they write. The last value keeps the runtime
/// and its arena alive.
pub(crate) fn screen_operations(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
) -> (PendingOffscreen, wgpu::Texture, Box<dyn std::any::Any>) {
    let (mut runtime, arena) = runtime_on(device, authority);
    let mut state = OffscreenState::new(&runtime);
    let mut frame = clear();
    frame.render_textures[0].graphics_format = 4;
    frame.render_textures[0].srgb = true;
    let mut operations = state.build(&mut runtime, &arena, &frame).unwrap();
    operations.export(TARGET, 0, 0, crate::GAME_SCREEN).unwrap();
    let handle = state.texture_handle(TARGET).unwrap();
    let texture = runtime.gpu_texture(handle).unwrap();
    (
        operations.into_pending(),
        texture,
        Box::new((runtime, arena)),
    )
}

/// The operations of [`clearing_operations`] on a texture stored as sRGB, as a camera's target is,
/// with the texture they write. They export nothing: the UI reads the texture. The last value
/// keeps the runtime and its arena alive.
pub(crate) fn target_operations(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
) -> (PendingOffscreen, wgpu::Texture, Box<dyn std::any::Any>) {
    let (mut runtime, arena) = runtime_on(device, authority);
    let mut state = OffscreenState::new(&runtime);
    let mut frame = clear();
    frame.render_textures[0].graphics_format = 4;
    frame.render_textures[0].srgb = true;
    let operations = state.build(&mut runtime, &arena, &frame).unwrap();
    let handle = state.texture_handle(TARGET).unwrap();
    let texture = runtime.gpu_texture(handle).unwrap();
    (
        operations.into_pending(),
        texture,
        Box::new((runtime, arena)),
    )
}

/// Whether `bytes` is the 8 by 8 render texture [`clear`] makes, all yellow.
pub(crate) fn is_cleared(bytes: &[u8]) -> bool {
    bytes.len() == 8 * 8 * 4 && bytes.chunks_exact(4).all(|texel| texel == YELLOW)
}

#[test]
fn offscreen_work_handed_to_the_host_runs_in_the_next_rendered_frame() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, image, _runtime) = clearing_operations(&device, &authority);

    let before = authority.submitted_serial();
    host.admit_offscreen(operations);
    render_frame(&mut host, &mut renderer);
    renderer.drain().unwrap();
    assert!(
        authority.submitted_serial() > before,
        "the frame went through the viewer's authority"
    );
    assert!(
        is_cleared(&image.bytes(&device)),
        "the camera cleared the render texture in the viewer's frame"
    );
}

#[test]
fn queued_fitting_disable_recovers_pending_offscreen_packet_without_readmission() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let fitted = crate::renderer::sdsm_tests::fitted_scene();
    let mut host = RetainedHost::new(
        fitted.clone(),
        crate::ViewerOptions {
            shadows: Some(super::shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    assert!(renderer.resident.depth_fitting_enabled());
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, image, _runtime) = clearing_operations(&device, &authority);
    host.admit_offscreen(operations);
    assert!(matches!(
        renderer.render_retained(&mut host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert!(host.offscreen.is_some());

    host.apply(crate::RetainedUpdate::Settings(Box::new(
        crate::renderer::retained_tests::source()
            .effects()
            .unwrap()
            .clone(),
    )))
    .unwrap();
    assert_eq!(host.accepted_generation(), 1);
    assert!(renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    assert_eq!(host.ready_generation(), 1);
    assert!(
        is_cleared(&image.bytes(&device)),
        "the original packet rendered after queued fitting was disabled"
    );
}

#[test]
fn accumulated_capture_rejects_offscreen_after_queued_fitting_publication() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [64, 64],
            shadows: Some(super::shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    assert!(!renderer.resident.depth_fitting_enabled());
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _image, _runtime) = clearing_operations(&device, &authority);
    host.admit_offscreen(operations);
    host.apply(crate::RetainedUpdate::Settings(Box::new(
        crate::renderer::sdsm_tests::fitted_scene()
            .effects()
            .unwrap()
            .clone(),
    )))
    .unwrap();
    let generation = host.accepted_generation();
    host.request_capture_recipe(
        generation,
        crate::RetainedCaptureRecipe::new(
            1,
            2,
            crate::CaptureJitter::Halton23,
            false,
            vec![crate::CaptureOutput::Srgb8],
            4 * 1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let before = authority.submitted_serial();

    assert!(matches!(
        renderer.render_retained(&mut host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert_eq!(host.ready_generation(), generation);
    assert!(renderer.resident.depth_fitting_enabled());
    assert_eq!(authority.submitted_serial(), before);
    assert!(host.offscreen.is_some());
}

#[test]
fn offscreen_work_refuses_depth_fitted_frame_before_submission() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let options = crate::ViewerOptions {
        shadows: Some(super::shadow_settings()),
        ..crate::tests::options()
    };
    let mut host = RetainedHost::new(crate::renderer::sdsm_tests::fitted_scene(), options).unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _image, _runtime) = clearing_operations(&device, &authority);
    let before = authority.submitted_serial();
    host.admit_offscreen(operations);

    assert!(matches!(
        renderer.render_retained(&mut host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert_eq!(authority.submitted_serial(), before);
    assert!(host.offscreen.is_some());
}
