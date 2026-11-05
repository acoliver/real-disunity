//! The view and projection of a camera, against points worked out by hand.

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::presentation::{Camera, Projection};

use super::super::builtins::{VIEW_PROJECTION, camera_layer};
use super::support::{IDENTITY, assert_close, matrix, transform};
use worldrunner_world_simulation::graphics::CameraView;

fn view(settings: Camera, position: Vec3, rotation: Quat) -> CameraView {
    CameraView {
        settings,
        world_matrix: Mat4::from_rotation_translation(rotation, position).to_cols_array(),
    }
}

fn perspective_90() -> Camera {
    Camera {
        projection: Projection::Perspective,
        field_of_view: 90.0,
        near: 1.0,
        far: 100.0,
        ..Camera::default()
    }
}

/// A world point in clip coordinates through the view-projection a camera supplies.
fn clip(view: &CameraView, aspect: f32, point: [f32; 3]) -> [f32; 4] {
    let layer = camera_layer(view, aspect);
    transform(
        &matrix(&layer, VIEW_PROJECTION),
        [point[0], point[1], point[2], 1.0],
    )
}

#[test]
fn a_perspective_camera_clips_a_point_as_worked_out_by_hand() {
    // The camera at (1, 2, 3) looks along +Z. The point (1.5, 2.5, 5) is 0.5 right, 0.5 up and 2
    // ahead. With a 90 degree field of view, f = 1, and an aspect of 2:
    //   x = f / aspect * 0.5 = 0.25,  y = f * 0.5 = 0.5,  w = 2,
    //   z (OpenGL) = -(far + near) / (far - near) * -2 - 2 * far * near / (far - near)
    //              = 202 / 99 - 200 / 99 = 2 / 99,
    // and the Vulkan depth is (z + w) / 2 = 1 + 1 / 99.
    let view = view(perspective_90(), Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
    let found = clip(&view, 2.0, [1.5, 2.5, 5.0]);
    assert_close(&found, &[0.25, 0.5, 1.0 + 1.0 / 99.0, 2.0]);
}

#[test]
fn depth_runs_from_0_at_the_near_plane_to_w_at_the_far_plane() {
    let view = view(perspective_90(), Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
    let near = clip(&view, 1.0, [1.0, 2.0, 4.0]);
    let far = clip(&view, 1.0, [1.0, 2.0, 103.0]);
    assert_close(&[near[2] / near[3]], &[0.0]);
    assert_close(&[far[2] / far[3]], &[1.0]);
}

#[test]
fn a_camera_turned_a_quarter_looks_along_x_and_its_right_is_negative_z() {
    // Turning +Z to +X about the up axis leaves a left handed camera's right at -Z.
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let view = view(perspective_90(), Vec3::new(1.0, 2.0, 3.0), rotation);
    let ahead = clip(&view, 1.0, [6.0, 2.0, 3.0]);
    assert_close(&ahead[..2], &[0.0, 0.0]);
    assert_close(&ahead[3..], &[5.0]);
    let right = clip(&view, 1.0, [6.0, 2.0, 2.0]);
    assert_close(&[right[0] / right[3]], &[0.2]);
}

#[test]
fn an_orthographic_camera_maps_its_half_height_and_aspect_to_the_clip_edges() {
    let settings = Camera {
        projection: Projection::Orthographic,
        half_height: 2.0,
        near: 0.5,
        far: 20.5,
        ..Camera::default()
    };
    let view = view(settings, Vec3::ZERO, Quat::IDENTITY);
    // Half the width is 4 at an aspect of 2. The edge at x = 4, y = 2 is clip (1, 1), and w stays 1.
    let found = clip(&view, 2.0, [4.0, 2.0, 10.5]);
    // z = 10.5 is the middle of the box from 0.5 to 20.5: OpenGL z = -2 / 20 * -10.5 - 21 / 20 = 0,
    // and Vulkan (z + 1) / 2 = 0.5.
    assert_close(&found, &[1.0, 1.0, 0.5, 1.0]);
}

#[test]
fn matrices_a_script_set_replace_the_derived_ones() {
    let settings = Camera {
        projection_matrix: Some(IDENTITY),
        world_to_camera: Some(IDENTITY),
        ..perspective_90()
    };
    let view = view(settings, Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY);
    // The view and the projection are the identity, so only the depth remap is left: (z + 1) / 2.
    let found = clip(&view, 1.0, [0.5, 0.25, 0.5]);
    assert_close(&found, &[0.5, 0.25, 0.75, 1.0]);
}

#[test]
fn the_scale_of_the_camera_object_does_not_change_the_view() {
    let rotation = Quat::from_rotation_y(0.3);
    let plain = view(perspective_90(), Vec3::new(1.0, 2.0, 3.0), rotation);
    let scaled = CameraView {
        settings: perspective_90(),
        world_matrix: Mat4::from_scale_rotation_translation(
            Vec3::new(3.0, 3.0, 3.0),
            rotation,
            Vec3::new(1.0, 2.0, 3.0),
        )
        .to_cols_array(),
    };
    let point = [4.0, 1.0, 9.0];
    assert_close(&clip(&scaled, 1.5, point), &clip(&plain, 1.5, point));
}
