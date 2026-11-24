//! The ambient default the host supplies, drawn on a real adapter (lavapipe in CI).
//!
//! The shader writes the fourth component of `unity_SHAr` as the grey of the quad it draws. No
//! layer sets the name unless a test does, so the draw reads the engine's default ambient. Every
//! expected texel comes from the default or the value the test sets, not from the output.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{OffscreenWork, PropertyValue, RenderTextureId};
use worldrunner_world_simulation::presentation::Camera;

use super::flush::host_with;
use super::gpu::{Quad, SIDE, target};
use super::lights_gpu::{Commands, filled, halves, quad, record, two_lists};
use super::support::{camera, set_global_in_earlier_take, snapshot, world};
use crate::offscreen::{Report, SH_AR, Settings};

pub(super) const ARGB32: u32 = 8;
/// The flat ambient the engine gives when nothing sets the names.
pub(super) const DEFAULT: f32 = 0.3;
const WORLD: f32 = 0.8;
const RECORDED: f32 = 0.6;

/// `found` equals `expected` to within one step of an 8-bit channel. 0.3 sits halfway between two
/// 8-bit values (76.5 of 255), and the adapter may round either way.
pub(super) fn assert_texels(found: &[f32], expected: &[f32]) {
    assert_eq!(found.len(), expected.len());
    for (index, (found, expected)) in found.iter().zip(expected).enumerate() {
        assert!(
            (found - expected).abs() <= 1.0 / 255.0 + 1.0e-5,
            "entry {index}: {found} against {expected}"
        );
    }
}

/// Flushes the world's frame and reads the whole of `drawn`, with the report of the flush.
pub(super) fn flush_and_read(mut world: World, drawn: RenderTextureId) -> (Vec<f32>, Report) {
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        ..Settings::default()
    };
    let mut host = host_with(directory.path(), settings);
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    (world.take_texture_pixels(request).unwrap(), outcome.report)
}

pub(super) fn camera_render(before: Commands<'_>) -> (World, RenderTextureId) {
    let mut world = world();
    before(&mut world);
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "ambient", 0, [1.0, 1.0, 0.0, 0.0]);
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

fn list_draws(before: Commands<'_>, between: Commands<'_>) -> (World, RenderTextureId) {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "ambient", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "ambient", 1, [0.5, 1.0, 0.5, 0.0]);
    two_lists(&mut world, drawn, (before, between));
    (world, drawn)
}

fn world_value(value: f32) -> PropertyValue {
    PropertyValue::Vector(SH_AR.into(), [0.0, 0.0, 0.0, value])
}

#[test]
fn a_camera_render_reads_the_default_ambient_and_the_name_is_not_left_missing() {
    let (world, drawn) = camera_render(&|_| {});
    let (read, report) = flush_and_read(world, drawn);
    assert_texels(&read, &filled([DEFAULT; 3]));
    assert!(!report.unset_constants.contains(SH_AR));
}

#[test]
fn a_list_draw_reads_the_default_ambient() {
    let (world, drawn) = list_draws(&|_| {}, &|_| {});
    let (read, report) = flush_and_read(world, drawn);
    assert_texels(&read, &filled([DEFAULT; 3]));
    assert!(!report.unset_constants.contains(SH_AR));
}

#[test]
fn the_worlds_own_value_wins_over_the_default_ambient() {
    // Written in an earlier take, the value is in the state the frame starts from.
    let (world, drawn) = camera_render(&|world| {
        set_global_in_earlier_take(world, world_value(WORLD));
    });
    assert_texels(&flush_and_read(world, drawn).0, &filled([WORLD; 3]));

    // Written in the frame before the first list, it is ordered in front of that list.
    let (world, drawn) = list_draws(
        &|world| world.set_global_property(world_value(WORLD)).unwrap(),
        &|_| {},
    );
    assert_texels(&flush_and_read(world, drawn).0, &filled([WORLD; 3]));
}

#[test]
fn a_value_recorded_between_two_list_draws_replaces_the_default_from_there() {
    let (world, drawn) = list_draws(&|_| {}, &|world| {
        record(world, OffscreenWork::SetGlobal(world_value(RECORDED)));
    });
    let read = flush_and_read(world, drawn).0;
    assert_texels(&read, &halves([DEFAULT; 3], [RECORDED; 3]));
}
