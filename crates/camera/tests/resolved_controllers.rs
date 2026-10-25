use glam::{DVec3, Vec3};
use worldrunner_camera::{CameraProfile, FreeFlyCamera, RtsCamera, ViewHandedness, Viewport};

fn profile() -> CameraProfile {
    CameraProfile::from_text(include_str!("fixtures/camera-profile-rts-v1.json")).unwrap()
}

#[test]
fn free_fly_and_rts_resolved_views_preserve_raster_and_reconstruct_landmarks() {
    let viewport = Viewport {
        width: 960,
        height: 720,
    };
    let aspect = f64::from(viewport.width) / f64::from(viewport.height);
    for handedness in [
        ViewHandedness::RightHanded,
        ViewHandedness::DirectXLeftHanded,
    ] {
        let mut flight = FreeFlyCamera::new(&profile(), DVec3::new(3.0, 10.0, 4.0), 0.3, -0.4, 1.1);
        flight.handedness = handedness;
        let rts = RtsCamera::from_free_fly_pose(flight.position, 0.3, -0.4, &profile())
            .unwrap()
            .with_handedness(handedness);
        let pairs = [
            (
                flight.camera_view(aspect, 0.5, 200.0),
                flight.resolved_view(viewport, 0.5, 200.0).unwrap(),
                (flight.position + flight.forward() * 10.0).as_vec3(),
            ),
            (
                rts.camera_view(aspect, 0.5, 200.0),
                rts.resolved_view(viewport, 0.5, 200.0).unwrap(),
                rts.look_at().as_vec3(),
            ),
        ];
        for (camera_view, resolved, landmark) in pairs {
            assert!(
                camera_view
                    .view_projection
                    .abs_diff_eq(resolved.view_projection_matrix(), 0.00001)
            );
            assert_eq!(camera_view.position, resolved.eye_meters());
            let pixel = resolved.world_to_screen(landmark + Vec3::Y).unwrap();
            assert!(
                resolved
                    .screen_to_world(pixel)
                    .unwrap()
                    .abs_diff_eq(landmark + Vec3::Y, 0.001)
            );
            let reconstruction =
                resolved.reconstruction_projection_matrix() * resolved.reconstruction_view_matrix();
            assert!(reconstruction.abs_diff_eq(resolved.view_projection_matrix(), 0.0001));
        }
    }
}
