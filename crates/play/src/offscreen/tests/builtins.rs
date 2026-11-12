//! The per-draw and camera values the host supplies.

use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::{CameraView, WorldLight};
use worldrunner_world_simulation::presentation::{Camera, LightKind, LightSettings};

use super::super::builtins::{
    CAMERA_POSITION, MAIN_LIGHT_COLOR, MAIN_LIGHT_POSITION, OBJECT_TO_WORLD, VIEW, WORLD_TO_OBJECT,
    add_main_light, camera_layer, draw_layer,
};
use super::super::frame::{Settings, adapt};
use super::support::{
    IDENTITY, assert_close, available, camera, matrix, product, render_texture, snapshot, sun,
    transform, world,
};
use worldrunner_render_program::ids::CameraId;
use worldrunner_world_simulation::graphics::OffscreenWork;

/// The bit patterns of a matrix, so that a copy is compared exactly.
fn bits(matrix: &[f32; 16]) -> [u32; 16] {
    matrix.map(f32::to_bits)
}

#[test]
fn the_object_to_world_matrix_is_the_renderers_and_its_inverse_undoes_it() {
    let local_to_world = Mat4::from_scale_rotation_translation(
        Vec3::new(2.0, 3.0, 0.5),
        Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.2),
        Vec3::new(-4.0, 5.0, 6.0),
    )
    .to_cols_array();
    let layer = draw_layer(&local_to_world);
    assert_eq!(
        bits(&matrix(&layer, OBJECT_TO_WORLD)),
        bits(&local_to_world)
    );
    let inverse = matrix(&layer, WORLD_TO_OBJECT);
    assert_close(&product(&inverse, &local_to_world), &IDENTITY);
    assert_close(&product(&local_to_world, &inverse), &IDENTITY);
    // A local point (1, 1, 1): scaled to (2, 3, 0.5), turned, then moved. Worked out by hand
    // for the world point and mapped back through the inverse.
    let world = transform(&local_to_world, [1.0, 1.0, 1.0, 1.0]);
    assert_close(&transform(&inverse, world), &[1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn a_matrix_with_no_inverse_has_no_world_to_object_value() {
    let flat = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0)).to_cols_array();
    let layer = draw_layer(&flat);
    assert_eq!(bits(&matrix(&layer, OBJECT_TO_WORLD)), bits(&flat));
    assert!(layer.values.get(WORLD_TO_OBJECT).is_none());
}

#[test]
fn a_draw_supplies_only_the_names_the_evidence_gives() {
    let layer = draw_layer(&IDENTITY);
    assert!(layer.values.get("unity_ObjectToWorld").is_some());
    assert!(layer.values.get("unity_MatrixPreviousM").is_none());
    assert!(layer.textures.is_empty());
}

#[test]
fn a_camera_supplies_its_view_and_position() {
    let view = CameraView {
        settings: Camera::default(),
        world_matrix: Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)).to_cols_array(),
    };
    let layer = camera_layer(&view, 1.0);
    // Facing +Z from (1, 2, 3), the point (1.5, 2.5, 5) is 0.5 right, 0.5 up and 2 ahead, and
    // the view looks down -Z.
    let found = transform(&matrix(&layer, VIEW), [1.5, 2.5, 5.0, 1.0]);
    assert_close(&found, &[0.5, 0.5, -2.0, 1.0]);
    assert_eq!(
        layer.values.get(CAMERA_POSITION),
        Some(&PropertyValue::Vector([1.0, 2.0, 3.0, 0.0]))
    );
}

#[test]
fn the_camera_position_follows_a_view_a_script_set() {
    // A view that moves the world by (-7, 0, 0) puts the camera at (7, 0, 0).
    let mut moved = IDENTITY;
    moved[12] = -7.0;
    let view = CameraView {
        settings: Camera {
            world_to_camera: Some(moved),
            ..Camera::default()
        },
        world_matrix: IDENTITY,
    };
    let layer = camera_layer(&view, 1.0);
    assert_eq!(
        layer.values.get(CAMERA_POSITION),
        Some(&PropertyValue::Vector([7.0, 0.0, 0.0, 0.0]))
    );
}

/// A light of `kind` turned by `rotation`, with the colour and intensity the expected values use.
fn light(kind: LightKind, rotation: Quat) -> WorldLight {
    WorldLight {
        settings: LightSettings {
            kind,
            color: [0.5, 0.25, 1.0, 0.75],
            intensity: 2.0,
            ..LightSettings::default()
        },
        world_matrix: Mat4::from_rotation_translation(rotation, Vec3::new(9.0, 9.0, 9.0))
            .to_cols_array(),
    }
}

fn vector(layer: &PropertyLayer, name: &str) -> [f32; 4] {
    match layer.values.get(name) {
        Some(PropertyValue::Vector(value)) => *value,
        other => panic!("{name} is {other:?}, not a vector"),
    }
}

#[test]
fn the_first_directional_light_gives_the_direction_toward_it_and_its_radiance() {
    // Turned 90 degrees about x, the light's forward axis (0, 0, 1) becomes (0, -1, 0): it shines
    // down, so the direction from a surface toward it is (0, 1, 0). The point light before it and
    // the directional light after it are not the main light.
    let lights = [
        light(LightKind::Point, Quat::IDENTITY),
        light(LightKind::Directional, Quat::from_rotation_x(FRAC_PI_2)),
        light(LightKind::Directional, Quat::IDENTITY),
    ];
    let mut layer = PropertyLayer::default();
    add_main_light(&mut layer, &lights);
    assert_close(&vector(&layer, MAIN_LIGHT_POSITION), &[0.0, 1.0, 0.0, 0.0]);
    // The colour times the intensity; the shaders read no other intensity.
    assert_close(&vector(&layer, MAIN_LIGHT_COLOR), &[1.0, 0.5, 2.0, 0.0]);
}

#[test]
fn the_direction_is_a_unit_vector_whatever_the_lights_scale() {
    let mut scaled = light(LightKind::Directional, Quat::IDENTITY);
    scaled.world_matrix = Mat4::from_scale(Vec3::splat(4.0)).to_cols_array();
    let mut layer = PropertyLayer::default();
    add_main_light(&mut layer, &[scaled]);
    assert_close(&vector(&layer, MAIN_LIGHT_POSITION), &[0.0, 0.0, -1.0, 0.0]);
}

#[test]
fn without_a_directional_light_with_a_direction_the_pair_stays_missing() {
    let mut flat = light(LightKind::Directional, Quat::IDENTITY);
    flat.world_matrix = Mat4::from_scale(Vec3::new(1.0, 1.0, 0.0)).to_cols_array();
    for lights in [
        vec![],
        vec![light(LightKind::Spot, Quat::IDENTITY)],
        vec![flat],
    ] {
        let mut layer = PropertyLayer::default();
        add_main_light(&mut layer, &lights);
        assert!(layer.values.get(MAIN_LIGHT_POSITION).is_none());
        assert!(layer.values.get(MAIN_LIGHT_COLOR).is_none());
    }
}

#[test]
fn a_camera_render_of_a_world_with_a_sun_carries_the_main_light() {
    let mut world = world();
    let target = render_texture(&mut world, [4, 4]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    sun(&mut world, Quat::from_rotation_x(FRAC_PI_2));
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    );
    let values = &adapted.frame.cameras[&CameraId(0)].values;
    assert_close(&vector(values, MAIN_LIGHT_POSITION), &[0.0, 1.0, 0.0, 0.0]);
    assert_close(&vector(values, MAIN_LIGHT_COLOR), &[1.0, 1.0, 1.0, 0.0]);
}
