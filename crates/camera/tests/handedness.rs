use std::time::Duration;

use glam::{DVec3, Mat4, Vec3};
use worldrunner_camera::{
    CameraProfile, CameraView, FreeFlyCamera, FreeFlyInput, RtsCamera, RtsCameraInput,
    ViewHandedness,
};

const BOTH_HANDEDNESS: [ViewHandedness; 2] = [
    ViewHandedness::RightHanded,
    ViewHandedness::DirectXLeftHanded,
];

fn ndc_x(view: CameraView, point: DVec3) -> f32 {
    let clip = view.view_projection * point.as_vec3().extend(1.0);
    assert!(clip.w > 0.0, "point must be in front of the camera");
    clip.x / clip.w
}

fn profile() -> CameraProfile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/camera-profile-rts-v1.json");
    CameraProfile::load(&path).expect("authored camera profile loads")
}

fn free_fly_facing_north(handedness: ViewHandedness) -> FreeFlyCamera {
    FreeFlyCamera {
        movement_speed_meters_per_second: 40.0,
        handedness,
        ..FreeFlyCamera::new(
            &profile(),
            DVec3::new(100.0, 50.0, -200.0),
            0.0,
            0.0,
            60.0_f64.to_radians(),
        )
    }
}

fn rts_facing_north(handedness: ViewHandedness) -> RtsCamera {
    RtsCamera::new(&profile())
        .expect("authored camera profile validates")
        .with_handedness(handedness)
}

#[test]
fn left_handed_view_projection_is_the_right_handed_one_with_clip_x_negated() {
    let mut right_handed = rts_facing_north(ViewHandedness::RightHanded);
    right_handed.yaw_radians = 0.4;
    let left_handed = right_handed.with_handedness(ViewHandedness::DirectXLeftHanded);
    let rh = right_handed
        .camera_view(16.0 / 9.0, 0.5, 5_000.0)
        .view_projection;
    let lh = left_handed
        .camera_view(16.0 / 9.0, 0.5, 5_000.0)
        .view_projection;
    let mirrored = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * rh;
    assert!(
        lh.abs_diff_eq(mirrored, 1.0e-4),
        "left-handed VP {lh:?} must equal the x-negated right-handed VP {mirrored:?}"
    );

    let right_handed_flight = free_fly_facing_north(ViewHandedness::RightHanded);
    let left_handed_flight = free_fly_facing_north(ViewHandedness::DirectXLeftHanded);
    let rh = right_handed_flight
        .camera_view(16.0 / 9.0, 0.5, 5_000.0)
        .view_projection;
    let lh = left_handed_flight
        .camera_view(16.0 / 9.0, 0.5, 5_000.0)
        .view_projection;
    assert!(lh.abs_diff_eq(Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * rh, 1.0e-4));
}

#[test]
fn reconstruction_projection_recovers_the_visible_raster_from_a_right_handed_view() {
    let camera = rts_facing_north(ViewHandedness::DirectXLeftHanded);
    let aspect = 16.0 / 9.0;
    let visible = camera.camera_view(aspect, 0.5, 5_000.0).view_projection;
    let horizontal_fov = camera.horizontal_fov_radians_for_aspect(aspect);
    let vertical_fov = 2.0 * ((horizontal_fov * 0.5).tan() / aspect).atan();
    let rh_projection =
        glam::dcamera::rh::proj::directx::perspective(vertical_fov, aspect, 0.5, 5_000.0).as_mat4();
    let rh_view =
        glam::dcamera::rh::view::look_at_mat4(camera.eye(), camera.look_at(), DVec3::Y).as_mat4();
    let reconstructed = camera.handedness.reconstruction_projection(rh_projection) * rh_view;
    assert!(reconstructed.abs_diff_eq(visible, 1.0e-3));
    assert_eq!(
        ViewHandedness::RightHanded.reconstruction_projection(rh_projection),
        rh_projection
    );
}

#[test]
fn left_handed_reconstruction_projection_is_the_right_handed_projection_mirrored_in_x() {
    let rh_projection = glam::camera::rh::proj::directx::perspective(1.1, 1.7, 0.1, 2_000.0);
    let mirrored = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * rh_projection;
    assert_eq!(
        ViewHandedness::DirectXLeftHanded
            .reconstruction_projection(rh_projection)
            .to_cols_array()
            .map(f32::to_bits),
        mirrored.to_cols_array().map(f32::to_bits)
    );
}

#[test]
fn left_handed_view_puts_native_west_on_screen_right_when_facing_north() {
    // A left-handed view maps the positive world X axis to screen-left.
    let free = free_fly_facing_north(ViewHandedness::DirectXLeftHanded);
    let view = free.camera_view(16.0 / 9.0, 0.5, 5_000.0);
    let ahead = free.position + DVec3::NEG_Z * 100.0;
    assert!(ndc_x(view, ahead + DVec3::NEG_X * 20.0) > 0.0);
    assert!(ndc_x(view, ahead + DVec3::X * 20.0) < 0.0);

    let rh = free_fly_facing_north(ViewHandedness::RightHanded);
    let view = rh.camera_view(16.0 / 9.0, 0.5, 5_000.0);
    assert!(ndc_x(view, ahead + DVec3::X * 20.0) > 0.0);
}

#[test]
fn rts_pan_right_moves_the_scene_toward_screen_left_under_both_handedness() {
    for handedness in BOTH_HANDEDNESS {
        let mut camera = rts_facing_north(handedness);
        let probe = camera.look_at() + DVec3::NEG_Z * 30.0;
        let before = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        camera.update(
            RtsCameraInput {
                right: 1.0,
                ..RtsCameraInput::default()
            },
            Duration::from_millis(100),
        );
        let after = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        assert!(
            after < before,
            "{handedness:?}: panning right must slide the world left on screen ({before} -> {after})"
        );
    }
}

#[test]
fn rts_pan_right_goes_west_when_left_handed_and_facing_north() {
    let mut camera = rts_facing_north(ViewHandedness::DirectXLeftHanded);
    camera.update(
        RtsCameraInput {
            right: 1.0,
            ..RtsCameraInput::default()
        },
        Duration::from_millis(250),
    );
    assert!(camera.ground_target.x < 0.0);
    assert!(camera.ground_target.z.abs() < 1.0e-12);
}

#[test]
fn free_fly_strafe_right_moves_the_scene_toward_screen_left_under_both_handedness() {
    for handedness in BOTH_HANDEDNESS {
        let mut camera = free_fly_facing_north(handedness);
        let probe = camera.position + DVec3::NEG_Z * 100.0;
        let before = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        camera.update(
            FreeFlyInput {
                right: 1.0,
                ..FreeFlyInput::default()
            },
            Duration::from_millis(100),
        );
        let after = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        assert!(
            after < before,
            "{handedness:?}: strafing right must slide the world left on screen ({before} -> {after})"
        );
    }
}

#[test]
fn free_fly_forward_and_up_do_not_depend_on_handedness() {
    let mut right_handed = free_fly_facing_north(ViewHandedness::RightHanded);
    let mut left_handed = free_fly_facing_north(ViewHandedness::DirectXLeftHanded);
    let input = FreeFlyInput {
        forward: 1.0,
        up: 1.0,
        ..FreeFlyInput::default()
    };
    right_handed.update(input, Duration::from_millis(100));
    left_handed.update(input, Duration::from_millis(100));
    assert!(
        right_handed
            .position
            .abs_diff_eq(left_handed.position, 1.0e-12)
    );
}

#[test]
fn positive_yaw_input_turns_the_view_toward_screen_left_under_both_handedness() {
    for handedness in BOTH_HANDEDNESS {
        let mut camera = free_fly_facing_north(handedness);
        let probe = camera.position + DVec3::NEG_Z * 100.0;
        let before = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        camera.update(
            FreeFlyInput {
                yaw_delta_radians: 0.1,
                ..FreeFlyInput::default()
            },
            Duration::ZERO,
        );
        let after = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        assert!(
            after > before,
            "{handedness:?}: turning the view left must slide the world right ({before} -> {after})"
        );
    }
}

#[test]
fn rts_rotate_left_turns_the_view_toward_screen_left_under_both_handedness() {
    for handedness in BOTH_HANDEDNESS {
        let mut camera = rts_facing_north(handedness);
        let probe = camera.look_at() + DVec3::NEG_Z * 30.0;
        let before = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        camera.update(
            RtsCameraInput {
                rotate: 1.0,
                ..RtsCameraInput::default()
            },
            Duration::from_millis(100),
        );
        let after = ndc_x(camera.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        assert!(
            after > before,
            "{handedness:?}: rotating left must slide the world right ({before} -> {after})"
        );

        let mut dragged = rts_facing_north(handedness);
        dragged.update(
            RtsCameraInput {
                yaw_delta_radians: 0.1,
                ..RtsCameraInput::default()
            },
            Duration::ZERO,
        );
        let dragged_x = ndc_x(dragged.camera_view(16.0 / 9.0, 0.5, 5_000.0), probe);
        assert!(dragged_x > before, "{handedness:?}: orbit yaw sign");
    }
}

#[test]
fn pitch_input_is_unchanged_by_handedness() {
    let input = RtsCameraInput {
        pitch_delta_radians: 0.05,
        tilt: 1.0,
        ..RtsCameraInput::default()
    };
    let mut right_handed = rts_facing_north(ViewHandedness::RightHanded);
    let mut left_handed = rts_facing_north(ViewHandedness::DirectXLeftHanded);
    right_handed.pitch_radians = -0.8;
    left_handed.pitch_radians = -0.8;
    right_handed.update(input, Duration::from_millis(50));
    left_handed.update(input, Duration::from_millis(50));
    assert!((right_handed.pitch_radians - left_handed.pitch_radians).abs() < f64::EPSILON);
}
