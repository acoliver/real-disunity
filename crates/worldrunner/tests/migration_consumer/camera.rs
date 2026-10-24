use worldrunner::glam::{DVec3, Vec3};
use worldrunner::scene::camera::{
    CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, ViewHandedness, Viewport,
    resolve,
};

pub fn planar(size: [u32; 2], handedness: ViewHandedness) -> ResolvedView {
    resolve(&CameraState {
        pose: CameraPose {
            eye_meters: DVec3::new(0.0, -10.0, 0.0),
            forward: DVec3::Y,
            up: DVec3::Z,
        },
        lens: CameraLens::Orthographic {
            half_height_meters: 2.0,
            near_meters: 0.0,
            far_meters: 20.0,
        },
        viewport: Viewport {
            width: size[0],
            height: size[1],
        },
        convention: ViewConvention { handedness },
    })
    .unwrap()
}

#[test]
fn planar_half_height_resize_and_top_left_queries_agree() {
    for size in [[65, 47], [130, 94]] {
        let view = planar(size, ViewHandedness::RightHanded);
        let point = Vec3::new(0.8, 0.0, 1.0);
        let screen = view.world_to_screen(point).unwrap();
        assert!((screen.y - f32::from(u16::try_from(size[1]).unwrap()) * 0.25).abs() < 0.001);
        assert!(
            view.screen_to_world(screen)
                .unwrap()
                .abs_diff_eq(point, 0.0001)
        );
        let hit = view
            .screen_ray(screen.truncate())
            .unwrap()
            .intersect_plane(Vec3::ZERO, Vec3::Y)
            .unwrap();
        assert!(hit.abs_diff_eq(point, 0.0001));
        assert_eq!(
            worldrunner::scene::gpu::SceneView::from_resolved(view).view_projection_matrix(),
            view.view_projection_matrix()
        );
    }
}

#[test]
fn free_fly_and_rts_reconstruct_their_projected_landmarks() {
    use worldrunner::scene::camera::{CameraProfile, FreeFlyCamera, RtsCamera};
    let profile = CameraProfile::from_text(include_str!(
        "../../../camera/tests/fixtures/camera-profile-rts-v1.json"
    ))
    .unwrap();
    let flight = FreeFlyCamera::new(&profile, DVec3::new(1.0, 20.0, 3.0), 0.3, -0.5, 1.0);
    let rts = RtsCamera::from_free_fly_pose(flight.position, 0.3, -0.5, &profile).unwrap();
    for view in [
        flight
            .resolved_view(
                Viewport {
                    width: 130,
                    height: 94,
                },
                0.5,
                200.0,
            )
            .unwrap(),
        rts.resolved_view(
            Viewport {
                width: 130,
                height: 94,
            },
            0.5,
            200.0,
        )
        .unwrap(),
    ] {
        let ray = view
            .screen_ray(worldrunner::glam::Vec2::new(65.0, 47.0))
            .unwrap();
        let landmark = ray.origin_meters + ray.direction * 10.0;
        let screen = view.world_to_screen(landmark).unwrap();
        assert!(
            screen
                .truncate()
                .abs_diff_eq(worldrunner::glam::Vec2::new(65.0, 47.0), 0.001),
            "screen={screen:?}, ray={ray:?}"
        );
        assert!(
            view.screen_to_world(screen)
                .unwrap()
                .abs_diff_eq(landmark, 0.001)
        );
        let raster = view.view_projection_matrix() * landmark.extend(1.0);
        let reconstructed = view.reconstruction_projection_matrix()
            * view.reconstruction_view_matrix()
            * landmark.extend(1.0);
        assert!((raster / raster.w).abs_diff_eq(reconstructed / reconstructed.w, 0.0001));
    }
}
