//! A global the run supplies by name (`play --global NAME=v0,v1,v2,v3`), drawn on a real adapter
//! (lavapipe in CI).
//!
//! The shader writes the member `unity_LightData` of a constant buffer as the colour of the quad
//! it draws. No light is in the world, so nothing sets the member and it packs as zero. Every
//! expected texel comes from the values the test supplies or records, not from the output.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{OffscreenWork, PropertyValue, RenderTextureId};
use worldrunner_world_simulation::presentation::Camera;

use super::flush::host_with;
use super::gpu::{Quad, SIDE, target};
use super::lights_gpu::{filled, halves, quad, record, two_lists};
use super::support::{assert_close, camera, snapshot, world};
use crate::offscreen::{Settings, SuppliedGlobal};

/// The member of the shader's constant buffer, named like a per-draw member.
pub(super) const NAME: &str = "unity_LightData";
const ARGB32: u32 = 8;
const SUPPLIED: [f32; 3] = [0.25, 0.5, 0.75];
const RECORDED: [f32; 3] = [0.8, 0.6, 0.2];

fn supplied(value: &str) -> Vec<SuppliedGlobal> {
    vec![SuppliedGlobal::parse(&format!("{NAME}={value}")).unwrap()]
}

fn set_global(world: &mut World, value: [f32; 3]) {
    let value = [value[0], value[1], value[2], 1.0];
    record(
        world,
        OffscreenWork::SetGlobal(PropertyValue::Vector(NAME.into(), value)),
    );
}

/// Flushes the world's frame on a host that starts from `supplied` and reads the whole of `drawn`.
fn flush_and_read(
    mut world: World,
    drawn: RenderTextureId,
    supplied: Vec<SuppliedGlobal>,
) -> Vec<f32> {
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        supplied,
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
    world.take_texture_pixels(request).unwrap()
}

/// A world whose one renderer fills the target, drawn by the camera after `before` is recorded.
fn camera_render(before: Option<[f32; 3]>) -> (World, RenderTextureId) {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "light_data", 0, [1.0, 1.0, 0.0, 0.0]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    if let Some(value) = before {
        set_global(&mut world, value);
    }
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
fn a_member_nothing_sets_reads_zero_when_the_run_supplies_nothing() {
    let (world, drawn) = camera_render(None);
    assert_close(&flush_and_read(world, drawn, Vec::new()), &filled([0.0; 3]));
}

#[test]
fn a_supplied_value_is_what_a_camera_render_reads_for_a_member_nothing_sets() {
    let (world, drawn) = camera_render(None);
    let read = flush_and_read(world, drawn, supplied("0.25,0.5,0.75,1"));
    assert_close(&read, &filled(SUPPLIED));
}

#[test]
fn a_supplied_value_with_fewer_floats_leaves_the_others_zero() {
    let (world, drawn) = camera_render(None);
    let read = flush_and_read(world, drawn, supplied("0.5,0.25"));
    assert_close(&read, &filled([0.5, 0.25, 0.0]));
}

#[test]
fn a_value_the_script_records_wins_over_the_supplied_one_for_a_camera_render() {
    let (world, drawn) = camera_render(Some(RECORDED));
    let read = flush_and_read(world, drawn, supplied("0.25,0.5,0.75,1"));
    assert_close(&read, &filled(RECORDED));
}

#[test]
fn a_supplied_value_holds_for_the_lists_until_the_script_records_one() {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "light_data", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "light_data", 1, [0.5, 1.0, 0.5, 0.0]);
    two_lists(
        &mut world,
        drawn,
        (&|_| {}, &|world| set_global(world, RECORDED)),
    );
    let read = flush_and_read(world, drawn, supplied("0.25,0.5,0.75,1"));
    // The first list draws with the supplied value, the second after the script recorded its own.
    assert_close(&read, &halves(SUPPLIED, RECORDED));
}
