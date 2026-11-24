//! A shader that declares a default for `_MainLightColor`, drawn on a real adapter (lavapipe in
//! CI). The declared default is the last value a draw reads for the name: a value the script
//! records, the world's own, one the run supplies and the host's main light all win over it, and
//! it is what the draw reads when none of them exists.
//!
//! Every expected texel comes from the declared default and the values the test gives the light,
//! the world, the run and the script, not from the output.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{OffscreenWork, PropertyValue, RenderTextureId};
use worldrunner_world_simulation::presentation::Camera;

use super::flush::host_with;
use super::gpu::{Quad, SIDE, target};
use super::lights_gpu::{
    COLOR, Commands, camera_renders, filled, halves, halves_world, quad, record, set_global,
    two_lists, unorm,
};
use super::support::{assert_close, camera, set_global_in_earlier_take, snapshot, world};
use crate::offscreen::{Settings, SuppliedGlobal};

/// What the shader declares for `_MainLightColor`.
pub(super) const DECLARED: [f32; 4] = [1.0, 0.0, 0.8, 1.0];
const ARGB32: u32 = 8;
/// A host light of colour (0.8, 0.4, 1.2) at half intensity shines (0.4, 0.2, 0.6).
const LIT: ([f32; 3], f32) = ([0.8, 0.4, 1.2], 0.5);
const HOST: [f32; 3] = [0.4, 0.2, 0.6];
const SUPPLIED: [f32; 3] = [0.25, 0.5, 0.75];
const WORLD: [f32; 3] = [0.2, 0.8, 0.4];
const RECORDED: [f32; 3] = [0.8, 0.6, 0.2];
const FILE: &str = "declared_light";

fn declared() -> [f32; 3] {
    [DECLARED[0], DECLARED[1], DECLARED[2]]
}

fn supplied() -> Vec<SuppliedGlobal> {
    vec![SuppliedGlobal::parse(&format!("{COLOR}=0.25,0.5,0.75,1")).unwrap()]
}

/// Flushes the world's frame on a host that starts from `supplied` and reads each of `targets`.
fn flush_and_read(
    world: World,
    targets: &[RenderTextureId],
    supplied: Vec<SuppliedGlobal>,
) -> Vec<Vec<f32>> {
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        supplied,
        ..Settings::default()
    };
    flush_with(world, targets, settings)
}

/// Flushes the world's frame on a host with `settings` and reads each of `targets`.
fn flush_with(mut world: World, targets: &[RenderTextureId], settings: Settings) -> Vec<Vec<f32>> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with(directory.path(), settings);
    let requests: Vec<_> = targets
        .iter()
        .map(|texture| {
            world
                .request_texture_pixels(*texture, 0, 0, SIDE, SIDE)
                .unwrap()
        })
        .collect();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    requests
        .into_iter()
        .map(|request| world.take_texture_pixels(request).unwrap())
        .collect()
}

/// A world with no light whose one renderer of the declaring shader the camera draws.
fn unlit_camera_render(before: Commands<'_>) -> (World, RenderTextureId) {
    unlit_camera_render_of(FILE, before)
}

/// The same, drawn with the shader `file`.
fn unlit_camera_render_of(file: &str, before: Commands<'_>) -> (World, RenderTextureId) {
    let mut world = world();
    before(&mut world);
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, file, 0, [1.0, 1.0, 0.0, 0.0]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: drawn,
        },
    );
    (world, drawn)
}

#[test]
fn the_declared_default_is_read_when_nothing_else_gives_the_colour_a_value() {
    let (world, drawn) = unlit_camera_render(&|_| {});
    let read = flush_and_read(world, &[drawn], Vec::new());
    assert_close(&read[0], &filled(declared()));
}

#[test]
fn the_hosts_main_light_wins_over_the_declared_default_for_a_camera_render() {
    let (world, targets) = camera_renders(FILE, LIT, (None, None));
    let read = flush_and_read(world, &targets, Vec::new());
    assert_close(&read[0], &filled(HOST));
    assert_close(&read[1], &filled(HOST));
}

#[test]
fn the_hosts_main_light_wins_over_the_declared_default_for_a_list_draw() {
    let (mut world, drawn) = halves_world(FILE, LIT);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    let read = flush_and_read(world, &[drawn], Vec::new());
    assert_close(&read[0], &filled(HOST));
}

#[test]
fn a_supplied_value_wins_over_the_declared_default_and_the_hosts_light() {
    let (world, drawn) = unlit_camera_render(&|_| {});
    let read = flush_and_read(world, &[drawn], supplied());
    assert_close(&read[0], &filled(SUPPLIED));

    let (world, targets) = camera_renders(FILE, LIT, (None, None));
    let read = flush_and_read(world, &targets, supplied());
    assert_close(&read[0], &filled(SUPPLIED));
    assert_close(&read[1], &filled(SUPPLIED));

    let (mut world, drawn) = halves_world(FILE, LIT);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    let read = flush_and_read(world, &[drawn], supplied());
    assert_close(&read[0], &filled(SUPPLIED));
}

#[test]
fn the_worlds_own_value_wins_over_the_declared_default_a_supplied_value_and_the_light() {
    let world_value = PropertyValue::Vector(COLOR.into(), [WORLD[0], WORLD[1], WORLD[2], 1.0]);

    // Written in an earlier take, the value is in the state the frame starts from.
    let (world, drawn) = unlit_camera_render(&|world| {
        set_global_in_earlier_take(world, world_value.clone());
    });
    let read = flush_and_read(world, &[drawn], Vec::new());
    assert_close(&read[0], &filled(WORLD));

    // Recorded in front of the first render, the value wins over the supplied one and the light.
    let (world, targets) = camera_renders(FILE, LIT, (Some(WORLD), None));
    let read = flush_and_read(world, &targets, supplied());
    assert_close(&read[0], &filled(WORLD));
    assert_close(&read[1], &filled(WORLD));

    // Written directly before the first list, it is ordered in front of that list.
    let (mut world, drawn) = halves_world(FILE, LIT);
    two_lists(
        &mut world,
        drawn,
        (
            &|world| world.set_global_property(world_value.clone()).unwrap(),
            &|_| {},
        ),
    );
    let read = flush_and_read(world, &[drawn], supplied());
    assert_close(&read[0], &filled(WORLD));
}

#[test]
fn a_value_recorded_between_a_camera_draw_and_a_list_draw_wins_over_the_declared_default() {
    let (mut world, drawn) = halves_world(FILE, LIT);
    // The unrelated camera render at the start of the pipeline reads the host's light; the
    // script then records its colour, and the first list reads that. The second list records its
    // own.
    two_lists(
        &mut world,
        drawn,
        (&|world| set_global(world, COLOR, RECORDED), &|_| {}),
    );
    let read = flush_and_read(world, &[drawn], Vec::new());
    assert_close(&read[0], &halves(RECORDED, RECORDED));

    let (world, targets) = camera_renders(FILE, LIT, (None, Some(RECORDED)));
    let read = flush_and_read(world, &targets, Vec::new());
    assert_close(&read[0], &filled(HOST));
    assert_close(&read[1], &filled(RECORDED));
}

#[test]
fn a_value_recorded_over_a_supplied_one_wins_over_the_declared_default() {
    let (world, targets) = camera_renders(FILE, LIT, (Some(RECORDED), None));
    let read = flush_and_read(world, &targets, supplied());
    assert_close(&read[0], &filled(RECORDED));
    assert_close(&read[1], &filled(RECORDED));
}

/// The sRGB decode of each of the declared default's red, green and blue, as the 8-bit target
/// stores it: 1 and 0 are unchanged and 0.8 is `((0.8 + 0.055) / 1.055)^2.4 = 0.603_827_3`.
fn declared_linear() -> [f32; 3] {
    [1.0, 0.0, unorm(0.603_827_3)]
}

/// Settings for a project in `linear` or Gamma colour space.
fn project(linear: bool) -> Settings {
    Settings {
        light_modes: vec!["Alpha".to_owned()],
        linear_colour_space: linear,
        ..Settings::default()
    }
}

#[test]
fn a_declared_colour_default_is_read_as_its_linear_form_in_a_linear_project() {
    let (world, drawn) = unlit_camera_render(&|_| {});
    let read = flush_with(world, &[drawn], project(true));
    assert_close(&read[0], &filled(declared_linear()));
}

#[test]
fn a_declared_colour_default_is_read_as_authored_in_a_gamma_project() {
    let (world, drawn) = unlit_camera_render(&|_| {});
    let read = flush_with(world, &[drawn], project(false));
    assert_close(&read[0], &filled(declared()));
}

#[test]
fn a_declared_vector_default_is_never_converted() {
    for linear in [true, false] {
        let (world, drawn) = unlit_camera_render_of("declared_vector", &|_| {});
        let read = flush_with(world, &[drawn], project(linear));
        assert_close(&read[0], &filled(declared()));
    }
}

#[test]
fn a_value_the_world_sets_is_not_converted_again_in_a_linear_project() {
    let world_value = PropertyValue::Vector(COLOR.into(), [WORLD[0], WORLD[1], WORLD[2], 1.0]);
    let (world, drawn) = unlit_camera_render(&|world| {
        set_global_in_earlier_take(world, world_value.clone());
    });
    let read = flush_with(world, &[drawn], project(true));
    assert_close(&read[0], &filled(WORLD));
}
