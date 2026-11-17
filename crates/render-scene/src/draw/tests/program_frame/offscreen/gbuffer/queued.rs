//! Two mixed offscreen and G-buffer frames queued on one submission authority.
//!
//! Both frames are recorded, encoded and submitted before anything polls the device, maps a
//! readback or takes a timing. Each frame then reads back its own camera, blit and array element,
//! and its own copy of the HDR scene. The stock scene is the same in both frames, so the scene
//! checks show the card after each frame and say nothing about separate scene or G-buffer targets.

use super::super::super::rig::{BLACK, Fixture, GREEN, RED};
use super::super::{
    CAMERA_SHADER, CYAN, CameraId, MaterialId, OffscreenFrame, Player, Readbacks, ZERO,
    build_frame, check_offscreen_stages, frame, material, pixel, player,
};
use super::{assert_scene_counts, fixture, hdr_pixel};
use crate::test_support::SceneTestSession;
use worldrunner_render_plan::SceneOpaquePath;
use worldrunner_render_post::contracts::RasterId;

const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// Camera pixels of the second frame, which the camera clears to white and draws cyan over
/// columns 0 to 3 and magenta over columns 6 and 7. The culled band changes nothing.
const SECOND_CAMERA: [((usize, usize), [u8; 4]); 8] = [
    ((1, 0), CYAN),
    ((3, 7), CYAN),
    ((1, 3), CYAN),
    ((4, 0), WHITE),
    ((4, 4), WHITE),
    ((5, 4), WHITE),
    ((6, 2), MAGENTA),
    ((7, 7), MAGENTA),
];

/// Linear half float bytes of the red card and of the black clear, both opaque.
const SCENE_RED: [u8; 8] = [0, 60, 0, 0, 0, 0, 0, 60];
const SCENE_BLACK: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 60];

#[test]
fn queued_offscreen_frames_keep_their_results_with_fused_gbuffer_lighting() {
    assert_queued_frames(SceneOpaquePath::GBUFFER_RENDER);
}

#[test]
fn queued_offscreen_frames_keep_their_results_with_gbuffer_observation() {
    assert_queued_frames(SceneOpaquePath::GBUFFER_OBSERVATION);
}

/// What a submitted frame leaves for the checks that follow every submission.
struct Queued {
    readbacks: Readbacks,
    scene: wgpu::Buffer,
    operations: Vec<&'static str>,
}

/// The first frame with cyan and magenta renderers over a white clear. The blit inverts them to
/// red, green and opaque black, and the copy puts that in element 2 of the array.
fn second_frame() -> OffscreenFrame {
    let mut second = frame();
    for (id, color) in [(1, CYAN), (2, MAGENTA)] {
        second
            .materials
            .insert(MaterialId(id), material(CAMERA_SHADER, tint(color)));
    }
    second
        .cameras
        .get_mut(&CameraId(1))
        .expect("the frame's camera")
        .clear_color = Some(tint(WHITE).map(f64::from));
    second
}

/// The unit tint that renders as `color` in an 8 bit target.
fn tint(color: [u8; 4]) -> [f32; 4] {
    color.map(|channel| f32::from(channel) / 255.0)
}

/// What the blit's inversion pass makes of `color`: each channel flipped, alpha opaque.
fn inverted(color: [u8; 4]) -> [u8; 4] {
    [255 - color[0], 255 - color[1], 255 - color[2], 255]
}

fn operation_names(ids: impl Iterator<Item = RasterId>) -> Vec<&'static str> {
    ids.map(|id| id.stage.name).collect()
}

/// Records, encodes and submits one mixed frame with its own timing frame and copy of the HDR
/// scene. Nothing here polls the device or reads a result back.
fn submit_frame(
    fixture: &mut Fixture,
    player: &mut Player,
    frame: &OffscreenFrame,
    timing: u64,
    path: SceneOpaquePath,
) -> Queued {
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();
    let published = fixture.gpu.statistics().passes;
    let (operations, readbacks) = build_frame(player, frame);
    let mut recording = authority.begin(&device).unwrap();
    fixture.renderer.begin_timing(timing).unwrap();
    let admitted = operations.admit(&recording).unwrap();
    let names = operation_names(admitted.ids());
    fixture.gpu.admit_offscreen(admitted);
    fixture.encode(&mut recording).unwrap();
    let scene = fixture.copy_scene(&mut recording);
    let timed = fixture.renderer.renderer_mut();
    timed.finish_timing(&mut recording).unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording submits nothing"
    );
    assert_eq!(
        fixture.gpu.statistics().passes,
        published,
        "counts publish on submit"
    );
    assert!(
        !fixture.gpu.has_offscreen(),
        "encoding consumes the admitted work"
    );
    recording.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "one frame, one submission"
    );
    assert_scene_counts(fixture, path);
    Queued {
        readbacks,
        scene,
        operations: names,
    }
}

/// The camera and blit stages of the second frame, each pixel derived from the placements and
/// colors above and from the inversion, with the copy checked against the blit.
fn check_second_stages(readbacks: Readbacks, device: &wgpu::Device) {
    let camera = readbacks.camera.bytes(device);
    let blit = readbacks.blit.bytes(device);
    for (at, color) in SECOND_CAMERA {
        assert_eq!(pixel(&camera, at), color, "second camera at {at:?}");
        assert_eq!(pixel(&blit, at), inverted(color), "second blit at {at:?}");
    }
    assert_eq!(pixel(&blit, (1, 0)), RED);
    assert_eq!(pixel(&blit, (6, 2)), GREEN);
    assert_eq!(pixel(&blit, (4, 0)), BLACK);
    assert_eq!(readbacks.element.bytes(device), blit);
    assert!(
        readbacks
            .untouched
            .bytes(device)
            .chunks(4)
            .all(|texel| texel == ZERO)
    );
}

/// The HDR scene copy of one frame: the stock red card and the black around it.
fn check_scene(fixture: &Fixture, scene: &wgpu::Buffer) {
    let image = fixture.read(scene);
    assert_eq!(hdr_pixel(image.bytes(), 30, 20), SCENE_RED);
    assert_eq!(hdr_pixel(image.bytes(), 3, 3), SCENE_BLACK);
}

fn assert_queued_frames(path: SceneOpaquePath) {
    let _session = SceneTestSession::acquire();
    let mut fixture = fixture(path);
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();

    let first = submit_frame(&mut fixture, &mut player, &frame(), 11, path);
    let second = submit_frame(&mut fixture, &mut player, &second_frame(), 12, path);
    assert_eq!(
        authority.submitted_serial(),
        before + 2,
        "two frames, two submissions, before anything is read"
    );
    assert_eq!(first.operations, second.operations);

    check_offscreen_stages(first.readbacks, &device);
    check_second_stages(second.readbacks, &device);
    check_scene(&fixture, &first.scene);
    check_scene(&fixture, &second.scene);
    assert_eq!(fixture.renderer.dropped_timing_frames(), 0);
    assert_eq!(
        authority.submitted_serial(),
        before + 2,
        "captures submit nothing and the timings stay unread"
    );
}
