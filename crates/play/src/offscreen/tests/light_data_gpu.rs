//! The main light's factor `unity_LightData.z` on a real adapter (lavapipe in CI).
//!
//! The shader writes `_MainLightColor.rgb * unity_LightData.z` as the colour of the quad it draws.
//! Layer 0 is drawn over the left half of the target and layer 1 over the right half. The world's
//! one directional light has the colour `LIGHT`, and its culling mask decides which half it lights.
//! Every expected texel comes from the colour and the masks the test gives, not from the output.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{OffscreenWork, RenderTextureId};
use worldrunner_world_simulation::presentation::{Camera, Light, LightKind, LightSettings};
use worldrunner_world_simulation::{ComponentDesc, World};

use super::gpu::target;
use super::lights_gpu::{halves, quad, record, two_lists};
use super::supplied_gpu::flush_and_read;
use super::support::{assert_close, camera, object, world};

const ARGB32: u32 = 8;
const LIGHT: [f32; 3] = [0.2, 0.4, 0.6];
const BLACK: [f32; 3] = [0.0; 3];

/// A directional light of colour `LIGHT` that affects the layers of `culling_mask`.
fn light(world: &mut World, culling_mask: u32) {
    let object = object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    let settings = LightSettings {
        kind: LightKind::Directional,
        color: [LIGHT[0], LIGHT[1], LIGHT[2], 1.0],
        intensity: 1.0,
        culling_mask,
        ..LightSettings::default()
    };
    let light = Light {
        enabled: true,
        settings,
    };
    world
        .add_component(object, ComponentDesc::Light(light))
        .unwrap();
}

/// A world that draws layer 0 over the left half and layer 1 over the right half of a target,
/// with the light of `culling_mask`.
fn halves_world(culling_mask: u32) -> (World, RenderTextureId) {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "light_factor", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "light_factor", 1, [0.5, 1.0, 0.5, 0.0]);
    light(&mut world, culling_mask);
    (world, drawn)
}

#[test]
fn a_camera_draws_the_covered_layer_in_the_light_and_the_excluded_layer_black() {
    let (mut world, drawn) = halves_world(1);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: drawn,
        },
    );
    let read = flush_and_read(world, drawn, Vec::new());
    assert_close(&read, &halves(LIGHT, BLACK));
}

#[test]
fn a_light_that_covers_both_layers_lights_both() {
    let (mut world, drawn) = halves_world(0b11);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: drawn,
        },
    );
    let read = flush_and_read(world, drawn, Vec::new());
    assert_close(&read, &halves(LIGHT, LIGHT));
}

#[test]
fn a_list_draws_the_covered_layer_in_the_light_and_the_excluded_layer_black() {
    let (mut world, drawn) = halves_world(1);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    // The culling result of layer 0 picks the light and lights layer 0. The result of layer 1
    // shares no layer with the light, so it picks none and layer 1 reads zero.
    let read = flush_and_read(world, drawn, Vec::new());
    assert_close(&read, &halves(LIGHT, BLACK));
}

#[test]
fn a_list_lights_a_layer_the_lights_mask_covers() {
    let (mut world, drawn) = halves_world(0b11);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    let read = flush_and_read(world, drawn, Vec::new());
    assert_close(&read, &halves(LIGHT, LIGHT));
}
