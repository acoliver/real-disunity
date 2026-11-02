use glam::DVec3;
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ViewConvention, ViewHandedness, Viewport,
};
use worldrunner_scene_viewer::{OrbitCamera, OrbitLimits};

use crate::scene_transform::{
    CameraError, TransformError, orbit_limits, scene_camera, scene_transform,
};

const IDENTITY: [f32; 16] = [
    1., 0., 0., 0., //
    0., 1., 0., 0., //
    0., 0., 1., 0., //
    0., 0., 0., 1.,
];

fn close(actual: i64, expected: i64, tolerance: i64) -> bool {
    (actual - expected).abs() <= tolerance
}

fn with_translation(mut matrix: [f32; 16], at: [f32; 3]) -> [f32; 16] {
    matrix[12..15].copy_from_slice(&at);
    matrix
}

/// A matrix turning by `degrees` about axis `axis` (0 x, 1 y, 2 z), scaled by `scale`.
fn turned(axis: usize, degrees: f32, scale: f32) -> [f32; 16] {
    let (s, c) = degrees.to_radians().sin_cos();
    let mut matrix = [0.0; 16];
    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
    matrix[axis * 4 + axis] = scale;
    matrix[a * 4 + a] = c * scale;
    matrix[a * 4 + b] = s * scale;
    matrix[b * 4 + a] = -s * scale;
    matrix[b * 4 + b] = c * scale;
    matrix[15] = 1.0;
    matrix
}

#[test]
fn the_identity_matrix_places_the_mesh_at_the_origin_unturned_and_unscaled() {
    let transform = scene_transform(&IDENTITY).unwrap();
    assert_eq!(transform.position_millimeters, [0, 0, 0]);
    assert_eq!(transform.rotation, worldrunner_render_plan::SceneRotation::IDENTITY);
    assert_eq!(transform.scale_millionths, 1_000_000);
}

#[test]
fn the_translation_column_is_metres_and_becomes_millimeters() {
    let transform = scene_transform(&with_translation(IDENTITY, [1.5, -2.0, 0.0004])).unwrap();
    assert_eq!(transform.position_millimeters, [1500, -2000, 0]);
}

#[test]
fn a_quarter_turn_about_each_axis_becomes_the_matching_quaternion() {
    let half = 707_107;
    for (axis, expected) in [(0, [half, 0, 0]), (1, [0, half, 0]), (2, [0, 0, half])] {
        let rotation = scene_transform(&turned(axis, 90.0, 1.0)).unwrap().rotation;
        assert!(
            close(rotation.x, expected[0], 2)
                && close(rotation.y, expected[1], 2)
                && close(rotation.z, expected[2], 2)
                && close(rotation.w, half, 2),
            "axis {axis}: {rotation:?}"
        );
    }
}

#[test]
fn the_opposite_turn_flips_the_vector_part_and_keeps_w_positive() {
    let rotation = scene_transform(&turned(2, -90.0, 1.0)).unwrap().rotation;
    assert!(close(rotation.z, -707_107, 2), "{rotation:?}");
    assert!(rotation.w > 0, "{rotation:?}");
}

#[test]
fn a_half_turn_keeps_a_normalised_quaternion() {
    for axis in 0..3 {
        let transform = scene_transform(&turned(axis, 180.0, 1.0)).unwrap();
        assert!(transform.validate().is_ok(), "axis {axis}");
    }
}

#[test]
fn a_uniform_scale_is_read_in_millionths_and_leaves_the_rotation_alone() {
    let transform = scene_transform(&turned(2, 90.0, 2.5)).unwrap();
    assert_eq!(transform.scale_millionths, 2_500_000);
    assert!(close(transform.rotation.z, 707_107, 2));
}

#[test]
fn turn_scale_and_translation_combine() {
    let matrix = with_translation(turned(1, 90.0, 0.5), [10.0, 20.0, 30.0]);
    let transform = scene_transform(&matrix).unwrap();
    assert_eq!(transform.position_millimeters, [10_000, 20_000, 30_000]);
    assert_eq!(transform.scale_millionths, 500_000);
    assert!(close(transform.rotation.y, 707_107, 2));
}

#[test]
fn a_matrix_with_shear_is_rejected() {
    let mut matrix = IDENTITY;
    matrix[4] = 0.5;
    assert_eq!(scene_transform(&matrix), Err(TransformError::Shear));
}

#[test]
fn a_zero_scale_is_rejected() {
    let mut matrix = IDENTITY;
    matrix[0] = 0.0;
    matrix[5] = 0.0;
    matrix[10] = 0.0;
    assert_eq!(scene_transform(&matrix), Err(TransformError::ZeroScale));
}

#[test]
fn different_scales_per_axis_are_rejected_as_the_scene_scales_uniformly() {
    let mut matrix = IDENTITY;
    matrix[0] = 2.0;
    assert_eq!(scene_transform(&matrix), Err(TransformError::NonUniformScale));
}

#[test]
fn a_mirroring_matrix_is_rejected() {
    let mut matrix = IDENTITY;
    matrix[0] = -1.0;
    assert_eq!(scene_transform(&matrix), Err(TransformError::Mirrored));
}

#[test]
fn a_position_beyond_the_scene_range_is_rejected() {
    let matrix = with_translation(IDENTITY, [f32::INFINITY, 0.0, 0.0]);
    assert_eq!(scene_transform(&matrix), Err(TransformError::OutOfRange));
}

fn state(eye: [f64; 3], forward: [f64; 3], lens: CameraLens) -> CameraState {
    CameraState {
        pose: CameraPose {
            eye_meters: DVec3::from(eye),
            forward: DVec3::from(forward).normalize(),
            up: DVec3::Z,
        },
        lens,
        viewport: Viewport {
            width: 960,
            height: 540,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::DirectXLeftHanded,
        },
    }
}

fn perspective(fov_degrees: f64) -> CameraLens {
    CameraLens::Perspective {
        vertical_fov_radians: fov_degrees.to_radians(),
        near_meters: 0.3,
        far_meters: 1000.0,
    }
}

fn limits() -> OrbitLimits {
    orbit_limits()
}

#[test]
fn the_lens_becomes_millidegrees_and_millimeters() {
    let (_, lens) = scene_camera(&state([0., -10., 10.], [0., 1., -1.], perspective(60.))).unwrap();
    assert_eq!(lens.vertical_fov_millidegrees, 60_000);
    assert_eq!(lens.near_plane_millimeters, 300);
    assert_eq!(lens.far_plane_millimeters, 1_000_000);
}

#[test]
fn the_eye_is_in_millimeters_and_the_target_is_where_the_view_meets_the_ground() {
    let (pose, _) = scene_camera(&state([1., -10., 10.], [0., 1., -1.], perspective(60.))).unwrap();
    assert_eq!(pose.eye_millimeters, [1000, -10_000, 10_000]);
    assert!(close(pose.target_millimeters[0], 1000, 2), "{pose:?}");
    assert!(close(pose.target_millimeters[1], 0, 2), "{pose:?}");
    assert!(close(pose.target_millimeters[2], 0, 2), "{pose:?}");
}

#[test]
fn a_level_view_targets_a_point_ten_metres_ahead() {
    let (pose, _) = scene_camera(&state([0., 0., 5.], [0., 1., 0.], perspective(60.))).unwrap();
    assert!(close(pose.target_millimeters[1], 10_000, 2), "{pose:?}");
    assert!(close(pose.target_millimeters[2], 5_000, 2), "{pose:?}");
}

#[test]
fn a_view_below_the_ground_plane_targets_a_point_ten_metres_ahead() {
    let (pose, _) = scene_camera(&state([0., 0., -5.], [0., 1., -1.], perspective(60.))).unwrap();
    assert!(close(pose.target_millimeters[1], 7_071, 3), "{pose:?}");
    assert!(close(pose.target_millimeters[2], -12_071, 3), "{pose:?}");
}

#[test]
fn every_converted_pose_is_one_the_orbit_controls_accept() {
    let views = [
        ([0., -10., 10.], [0., 1., -1.]),
        ([0., 0., 50.], [0., 0., -1.]),
        ([3., 4., 80.], [0.0001, 0.0001, -1.]),
        ([0., 0., 5.], [0., 1., 0.]),
        ([0., 0., 5.], [0., 1., 0.4]),
        ([0., 0., 5000.], [1., 1., -0.2]),
    ];
    for (eye, forward) in views {
        let (pose, _) = scene_camera(&state(eye, forward, perspective(45.))).unwrap();
        assert!(
            OrbitCamera::new(pose, limits()).is_ok(),
            "{eye:?} looking along {forward:?} gave {pose:?}"
        );
    }
}

#[test]
fn a_view_straight_down_keeps_the_eye_above_the_ground_point_within_the_orbit_elevation() {
    let (pose, _) = scene_camera(&state([0., 0., 50.], [0., 0., -1.], perspective(45.))).unwrap();
    assert_eq!(pose.eye_millimeters, [0, 0, 50_000]);
    assert!(pose.target_millimeters[2] < 50_000, "{pose:?}");
}

#[test]
fn an_orthographic_camera_cannot_be_shown() {
    let lens = CameraLens::Orthographic {
        half_height_meters: 5.0,
        near_meters: 0.3,
        far_meters: 100.0,
    };
    assert_eq!(
        scene_camera(&state([0., 0., 5.], [0., 1., 0.], lens)),
        Err(CameraError::Orthographic)
    );
}

#[test]
fn a_lens_beyond_the_scene_range_is_clamped_into_it() {
    let lens = CameraLens::Perspective {
        vertical_fov_radians: 3.2,
        near_meters: 0.0,
        far_meters: 1.0e12,
    };
    let (_, scene) = scene_camera(&state([0., 0., 5.], [0., 1., 0.], lens)).unwrap();
    assert_eq!(scene.vertical_fov_millidegrees, 179_999);
    assert_eq!(scene.near_plane_millimeters, 1);
    assert_eq!(scene.far_plane_millimeters, u32::MAX);
    assert!(scene.validate().is_ok());
}
