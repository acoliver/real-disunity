//! The projection, screen and orthographic parameters of a view, against values worked out from
//! the camera settings or the projection the script set.
//!
//! The Old World player's frames hold `_ProjectionParams` as (1, near, far, 1 / far),
//! `_ScreenParams` as (width, height, 1 + 1 / width, 1 + 1 / height) and `unity_OrthoParams` as
//! (half width, half height, 0, kind) with the half height of 5 and the half width of 5 times the
//! 16 by 9 aspect on its perspective camera (board 237, post 6904).

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::ids::CameraId;
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::presentation::{Camera, Projection};

use super::super::builtins::{ORTHO_PARAMS, PROJECTION_PARAMS, SCREEN_PARAMS};
use super::super::camera::projection;
use super::super::frame::{Settings, adapt};
use super::lists::{draw, drawing, lists, placed, run, scene, shaders};
use super::support::{available, camera, render_texture, snapshot, world};

/// The three vectors of a view, in the order the tests name them.
struct Params {
    projection: [f32; 4],
    screen: [f32; 4],
    ortho: [f32; 4],
}

fn params(layer: &PropertyLayer) -> Params {
    let vector = |name: &str| match layer.values.get(name) {
        Some(PropertyValue::Vector(value)) => *value,
        other => panic!("{name} is {other:?}, not a vector"),
    };
    Params {
        projection: vector(PROJECTION_PARAMS),
        screen: vector(SCREEN_PARAMS),
        ortho: vector(ORTHO_PARAMS),
    }
}

/// Compares to a relative tolerance of 1e-3. A far plane derived from the projection matrix loses
/// digits to the cancellation in its z scale, so 210 comes back as 209.9977 in the original's
/// frames.
fn assert_near(found: [f32; 4], expected: [f32; 4]) {
    for (index, (found, expected)) in found.iter().zip(expected).enumerate() {
        let allowed = 1.0e-3 * expected.abs().max(1.0);
        assert!(
            (found - expected).abs() <= allowed,
            "entry {index}: {found} against {expected} in {found:?} and {expected:?}"
        );
    }
}

/// The values a camera render of `settings` into a texture of `size` carries.
fn camera_render(settings: Camera, size: [u32; 2]) -> Params {
    let mut world = world();
    let target = render_texture(&mut world, size);
    let camera = camera(&mut world, settings, Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    );
    params(&adapted.frame.cameras[&CameraId(0)].values)
}

#[test]
fn a_perspective_camera_render_carries_its_planes_screen_and_orthographic_size() {
    let settings = Camera {
        projection: Projection::Perspective,
        field_of_view: 60.0,
        near: 0.3,
        far: 210.0,
        ..Camera::default()
    };
    let found = camera_render(settings, [16, 8]);
    assert_near(found.projection, [1.0, 0.3, 210.0, 1.0 / 210.0]);
    assert_near(found.screen, [16.0, 8.0, 1.0 + 1.0 / 16.0, 1.0 + 1.0 / 8.0]);
    // The camera keeps its orthographic half height of 5 while it draws in perspective: the half
    // width is 5 times the aspect of 2 and the kind is 0.
    assert_near(found.ortho, [10.0, 5.0, 0.0, 0.0]);
}

#[test]
fn an_orthographic_camera_render_carries_its_planes_screen_and_half_sizes() {
    let settings = Camera {
        projection: Projection::Orthographic,
        half_height: 2.0,
        near: 0.5,
        far: 100.0,
        ..Camera::default()
    };
    let found = camera_render(settings, [8, 4]);
    assert_near(found.projection, [1.0, 0.5, 100.0, 0.01]);
    assert_near(found.screen, [8.0, 4.0, 1.125, 1.25]);
    assert_near(found.ortho, [4.0, 2.0, 0.0, 1.0]);
}

/// The values of the first list drawn after the script set the projection of `settings` for an
/// aspect of 1 over a camera set up with the identity, bound to a target of 8 by 8.
fn explicit_projection(settings: &Camera) -> Params {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    world
        .record_offscreen(OffscreenWork::SetViewProjection {
            view: Mat4::IDENTITY.to_cols_array(),
            projection: projection(settings, 1.0).to_cols_array(),
        })
        .unwrap();
    draw(&mut world, drawing(&["UniversalForward"]));
    let adapted = run(&mut world, |_| {});
    params(&lists(&adapted)[0].values)
}

#[test]
fn a_list_drawn_with_an_explicit_perspective_projection_takes_its_planes_from_the_matrix() {
    let settings = Camera {
        projection: Projection::Perspective,
        field_of_view: 45.0,
        near: 0.3,
        far: 1000.0,
        ..Camera::default()
    };
    let found = explicit_projection(&settings);
    assert_near(found.projection, [1.0, 0.3, 1000.0, 0.001]);
    assert_near(found.screen, [8.0, 8.0, 1.125, 1.125]);
    // The camera the script set up holds the orthographic half height 5, which a perspective
    // matrix does not carry; the aspect of the matrix is 1.
    assert_near(found.ortho, [5.0, 5.0, 0.0, 0.0]);
}

#[test]
fn a_list_drawn_with_an_explicit_orthographic_projection_takes_its_half_sizes_from_the_matrix() {
    let settings = Camera {
        projection: Projection::Orthographic,
        half_height: 3.0,
        near: 0.5,
        far: 50.0,
        ..Camera::default()
    };
    let found = explicit_projection(&settings);
    assert_near(found.projection, [1.0, 0.5, 50.0, 0.02]);
    assert_near(found.ortho, [3.0, 3.0, 0.0, 1.0]);
}
