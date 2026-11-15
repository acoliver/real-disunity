//! Draws into the screen on a real adapter (lavapipe in CI): a command buffer binds the camera
//! target, clears it and draws a mesh over part of it, and the host reads the screen back as an
//! image with its rows top first.
//!
//! The shaders and meshes are those of the mesh draw tests. Every expected texel comes from the
//! clear colour and the rectangle drawn, with a channel value of 0 or 1 that any encoding keeps.

use std::path::Path;

use glam::Quat;

use super::super::super::adapter::OffscreenAdapter;
use super::super::super::compute::ComputeAdapter;
use super::super::super::flush::{FlushOutcome, OffscreenHost};
use super::super::super::frame::Settings;
use super::super::super::frame::Skip;
use super::super::super::kernels::PlayerKernels;
use super::super::super::pixels::REFUSED_NO_TEXTURE;
use super::super::super::screen::ScreenImage;
use super::super::gpu::SIDE;
use super::super::support::{object, render_texture};
use super::{
    ARGB32, BLUE, GREEN, MESH, Mat4, Meshes, OTHER_MESH, RED, Rectangle, Shaders, Vec3, World,
    bind_and_clear, draw_mesh, flat, gpu, gpu_with_budget, identity_view, mesh_of, record,
    snapshot, stored, target, world,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle};

/// The upper half of clip space, which is the top half of the picture.
const TOP: Rectangle = Rectangle {
    x: (-1.0, 1.0),
    y: (0.0, 1.0),
};
/// The right half of clip space.
const RIGHT: Rectangle = Rectangle {
    x: (0.0, 1.0),
    y: (-1.0, 1.0),
};

fn host_with(directory: &Path, screen: Option<[u32; 2]>) -> OffscreenHost {
    host_on(directory, screen, gpu())
}

/// As `host_with`, on a runtime that may hold `budget` bytes.
fn host_with_budget(directory: &Path, screen: [u32; 2], budget: u64) -> OffscreenHost {
    host_on(directory, Some(screen), gpu_with_budget(budget))
}

fn host_on(
    directory: &Path,
    screen: Option<[u32; 2]>,
    gpu: super::super::gpu::Gpu,
) -> OffscreenHost {
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        screen,
        ..Settings::default()
    };
    OffscreenHost::new(
        gpu.runtime,
        gpu.arena,
        OffscreenAdapter::with_supply(Shaders, settings),
        ComputeAdapter::new(PlayerKernels::new(directory)),
    )
}

fn bind_screen(world: &mut World) {
    bind_screen_depth(world, None);
}

/// Binds the camera target for colour, with `depth` as the depth target.
fn bind_screen_depth(world: &mut World, depth: Option<TargetRef>) {
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::CameraTarget],
            depth,
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
}

fn clear_screen(world: &mut World, value: [f32; 4]) {
    clear(world, (true, true), value);
}

/// Clears the bound target's colour and depth as `parts` say.
fn clear(world: &mut World, (color, depth): (bool, bool), value: [f32; 4]) {
    record(
        world,
        OffscreenWork::ClearRenderTarget {
            color,
            depth,
            stencil: false,
            value,
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
}

fn flush(host: &mut OffscreenHost, world: &mut World, meshes: &Meshes) -> FlushOutcome {
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, meshes).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert!(
        outcome.report.skipped.is_empty(),
        "{:?}",
        outcome.report.skipped
    );
    outcome
}

/// The image's texels, row by row from the top.
fn rows(image: &ScreenImage) -> Vec<Vec<[u8; 4]>> {
    (0..image.height)
        .map(|y| (0..image.width).map(|x| image.pixel(x, y)).collect())
        .collect()
}

#[test]
fn a_draw_into_the_top_half_of_the_screen_reads_back_in_the_top_rows() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let mut world = world();
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    flush(&mut host, &mut world, &stored([(MESH, mesh_of(&[TOP]))]));
    assert!(host.screen_written());
    let image = host.read_screen(3).unwrap();

    assert_eq!((image.width, image.height, image.frame), (4, 4, 3));
    assert_eq!(
        rows(&image),
        [[RED; 4], [RED; 4], [GREEN; 4], [GREEN; 4]],
        "the picture's top rows hold what was drawn in the top half of clip space"
    );
    assert!(!host.screen_written(), "the read took what was written");
}

#[test]
fn a_later_flush_that_does_not_clear_keeps_what_the_earlier_one_left() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let meshes = stored([(MESH, mesh_of(&[TOP])), (OTHER_MESH, mesh_of(&[RIGHT]))]);
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    identity_view(&mut world);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    flush(&mut host, &mut world, &meshes);
    host.read_screen(0).unwrap();

    bind_screen(&mut world);
    identity_view(&mut world);
    draw_mesh(&mut world, (OTHER_MESH, Mat4::IDENTITY), blue, (0, -1));
    flush(&mut host, &mut world, &meshes);
    let image = host.read_screen(1).unwrap();

    let (top, bottom) = ([RED, RED, BLUE, BLUE], [GREEN, GREEN, BLUE, BLUE]);
    assert_eq!(rows(&image), [top, top, bottom, bottom]);
    assert_eq!(image.frame, 1);
}

#[test]
fn a_new_screen_size_makes_the_screen_again_at_that_size_and_empty() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let mut world = world();
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    flush(&mut host, &mut world, &stored([]));
    assert_eq!(host.read_screen(0).unwrap().width, 4);

    host.set_screen(Some([2, 3]));
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 0.0, 1.0, 1.0]);
    flush(&mut host, &mut world, &stored([]));
    let image = host.read_screen(1).unwrap();

    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(rows(&image), [[BLUE; 2]; 3]);
}

#[test]
fn work_that_draws_elsewhere_does_not_write_the_screen() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let (mut world, _) = super::scene();
    flush(&mut host, &mut world, &stored([]));
    assert!(!host.screen_written());
}

#[test]
fn a_host_with_no_screen_has_none_to_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), None);
    let mut world = world();
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &stored([])).unwrap();
    assert!(!host.screen_written());
    assert_eq!(outcome.report.skipped.values().sum::<usize>(), 2);
    assert_eq!(host.read_screen(0), Err(REFUSED_NO_TEXTURE.to_owned()));
}

/// The translation that puts a draw behind one at the identity: the shaders' less-or-equal depth
/// test fails where the nearer draw wrote its depth.
fn behind() -> Mat4 {
    Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
}

#[test]
fn a_draw_with_the_camera_target_as_depth_target_tests_against_the_screens_own_depth() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let meshes = stored([(MESH, mesh_of(&[TOP])), (OTHER_MESH, mesh_of(&[RIGHT]))]);
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    bind_screen_depth(&mut world, Some(TargetRef::CameraTarget));
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    identity_view(&mut world);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    draw_mesh(&mut world, (OTHER_MESH, behind()), blue, (0, -1));

    flush(&mut host, &mut world, &meshes);
    let image = host.read_screen(0).unwrap();

    let bottom = [GREEN, GREEN, BLUE, BLUE];
    assert_eq!(
        rows(&image),
        [[RED; 4], [RED; 4], bottom, bottom],
        "the blue draw behind the red one is hidden where the red one wrote depth"
    );
}

#[test]
fn the_screens_depth_persists_to_a_later_flush_that_does_not_clear() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let meshes = stored([(MESH, mesh_of(&[TOP])), (OTHER_MESH, mesh_of(&[RIGHT]))]);
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    bind_screen_depth(&mut world, Some(TargetRef::CameraTarget));
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    identity_view(&mut world);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    flush(&mut host, &mut world, &meshes);
    host.read_screen(0).unwrap();

    bind_screen_depth(&mut world, Some(TargetRef::CameraTarget));
    identity_view(&mut world);
    draw_mesh(&mut world, (OTHER_MESH, behind()), blue, (0, -1));
    flush(&mut host, &mut world, &meshes);
    let image = host.read_screen(1).unwrap();

    let bottom = [GREEN, GREEN, BLUE, BLUE];
    assert_eq!(rows(&image), [[RED; 4], [RED; 4], bottom, bottom]);
}

/// Binds `color` for colour and the camera target (the screen) for depth.
fn bind_screen_depth_under(world: &mut World, color: TargetRef) {
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![color],
            depth: Some(TargetRef::CameraTarget),
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
}

#[test]
fn a_clear_of_the_screens_depth_alone_does_not_write_its_colour() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let mut world = world();
    record(
        &mut world,
        OffscreenWork::SetRenderTarget {
            color: Vec::new(),
            depth: Some(TargetRef::CameraTarget),
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    bind_screen(&mut world);
    clear(&mut world, (false, true), [0.0, 1.0, 0.0, 1.0]);

    flush(&mut host, &mut world, &stored([]));

    assert!(!host.screen_written());
}

#[test]
fn a_draw_into_another_colour_target_with_the_screen_as_depth_does_not_write_the_screen() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([SIDE, SIDE]));
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    bind_screen_depth_under(&mut world, TargetRef::Render(drawn));
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    identity_view(&mut world);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    flush(&mut host, &mut world, &stored([(MESH, mesh_of(&[TOP]))]));

    assert!(!host.screen_written());
}

#[test]
fn a_flush_that_fails_after_a_submission_wrote_the_screen_still_marks_it_written() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    // No buffer holds the blocks the draw after the read packs, so the segment after the read
    // is refused; the clear before the read is its own segment and was submitted by then.
    host.set_constants_limit(8);
    let mut world = world();
    let asked = target(&mut world, ARGB32);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    world.request_texture_pixels(asked, 0, 0, 1, 1).unwrap();
    bind_screen(&mut world);
    identity_view(&mut world);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let snapshot = snapshot(&mut world);
    let result = host.flush(&snapshot, &stored([(MESH, mesh_of(&[TOP]))]));

    assert!(result.is_err(), "the draw's segment is refused");
    assert!(host.screen_written(), "the clear's segment was submitted");
}

#[test]
fn a_screen_taken_away_after_it_was_written_has_nothing_left_to_read() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let mut world = world();
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    flush(&mut host, &mut world, &stored([]));
    assert!(host.screen_written());

    host.set_screen(None);

    assert!(!host.screen_written());
    assert_eq!(host.read_screen(1), Err(REFUSED_NO_TEXTURE.to_owned()));
}

/// The camera component of `world`, a camera set up alone.
fn camera(world: &mut World) -> ComponentHandle {
    let object = object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

#[test]
fn a_camera_that_names_a_missing_screen_unbinds_the_target_so_a_later_clear_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), None);
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    bind_and_clear(&mut world, drawn);
    let camera = camera(&mut world);
    record(
        &mut world,
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::CameraTarget,
        },
    );
    clear_screen(&mut world, [0.0, 0.0, 1.0, 1.0]);
    let request = world.request_texture_pixels(drawn, 0, 0, 1, 1).unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &stored([])).unwrap();
    outcome.apply(&mut world).unwrap();

    assert_eq!(outcome.report.skipped.get(&Skip::UnboundTarget), Some(&1));
    assert_eq!(
        world.take_texture_pixels(request),
        Some(vec![0.0, 1.0, 0.0, 1.0]),
        "the clear that was refused left the texture green"
    );
}

/// Two flushes of a world, the second of which no longer names a render texture the first drew.
fn flushes_that_leave_an_unnamed_texture(host: &mut OffscreenHost) {
    let mut world = world();
    let large = render_texture(&mut world, [32, 32]);
    bind_and_clear(&mut world, large);
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    flush(host, &mut world, &stored([]));
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 0.0, 1.0, 1.0]);
    flush(host, &mut world, &stored([]));
    host.settle();
}

#[test]
fn a_screen_read_with_no_room_for_its_copy_takes_it_from_a_render_texture_no_snapshot_names() {
    let directory = tempfile::tempdir().unwrap();
    let mut roomy = host_with(directory.path(), Some([4, 4]));
    flushes_that_leave_an_unnamed_texture(&mut roomy);
    let held = roomy.live_bytes();

    // A budget of exactly what the flushes hold leaves nothing for the copy the read stages.
    let mut host = host_with_budget(directory.path(), [4, 4], held);
    flushes_that_leave_an_unnamed_texture(&mut host);
    let image = host.read_screen(2).unwrap();

    assert_eq!(rows(&image), [[BLUE; 4]; 4]);
}

#[test]
fn a_resized_screen_read_before_any_clear_holds_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), Some([4, 4]));
    let mut world = world();
    bind_screen(&mut world);
    clear_screen(&mut world, [0.0, 1.0, 0.0, 1.0]);
    flush(&mut host, &mut world, &stored([]));
    host.read_screen(0).unwrap();

    host.set_screen(Some([2, 3]));
    bind_screen(&mut world);
    clear(&mut world, (false, true), [0.0, 1.0, 0.0, 1.0]);
    flush(&mut host, &mut world, &stored([]));
    let image = host.read_screen(1).unwrap();

    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(rows(&image), [[[0; 4]; 2]; 3]);
}
