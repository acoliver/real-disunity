//! The per-draw and camera values the host supplies.

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::presentation::Camera;

use super::super::builtins::{
    CAMERA_POSITION, OBJECT_TO_WORLD, VIEW, WORLD_TO_OBJECT, camera_layer, draw_layer,
};
use super::support::{IDENTITY, assert_close, matrix, product, transform};
use worldrunner_world_simulation::graphics::CameraView;

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
