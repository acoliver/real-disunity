#![cfg(feature = "scene")]

use worldrunner::glam::{DVec3, Vec2, Vec3};
use worldrunner::scene::camera::{
    CameraLens, CameraPose, CameraProfile, CameraState, FreeFlyCamera, RtsCamera, ViewConvention,
    ViewHandedness, Viewport, resolve,
};
use worldrunner::scene::{gpu, plan};

#[test]
fn facade_camera_resolves_planar_screen_queries_for_existing_draw_contract() {
    let camera = resolve(&CameraState {
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
            width: 200,
            height: 100,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::RightHanded,
        },
    })
    .unwrap();
    let scene_view = gpu::SceneView::from_resolved(camera);
    let landmark = Vec3::new(2.0, 0.0, 1.0);
    let pixel = camera.world_to_screen(landmark).unwrap();
    assert!(pixel.abs_diff_eq(Vec3::new(150.0, 25.0, 0.5), 0.0001));
    let hit = scene_view
        .resolved_view()
        .screen_ray(Vec2::new(pixel.x, pixel.y))
        .unwrap()
        .intersect_plane(Vec3::ZERO, Vec3::Y)
        .unwrap();
    assert!(hit.abs_diff_eq(landmark, 0.0001));
    assert_eq!(
        scene_view.view_projection_matrix(),
        camera.view_projection_matrix()
    );
}

#[test]
fn facade_free_fly_rts_and_governed_views_share_reconstruction_contract() {
    let profile = CameraProfile::from_text(include_str!(
        "../../camera/tests/fixtures/camera-profile-rts-v1.json"
    ))
    .unwrap();
    let viewport = Viewport {
        width: 960,
        height: 720,
    };
    let flight = FreeFlyCamera::new(&profile, DVec3::new(1.0, 20.0, 3.0), 0.3, -0.5, 1.0);
    let rts = RtsCamera::from_free_fly_pose(flight.position, 0.3, -0.5, &profile).unwrap();
    for view in [
        flight.resolved_view(viewport, 0.5, 200.0).unwrap(),
        rts.resolved_view(viewport, 0.5, 200.0).unwrap(),
    ] {
        let raster = view.view_projection_matrix();
        let reconstruction =
            view.reconstruction_projection_matrix() * view.reconstruction_view_matrix();
        assert!(raster.abs_diff_eq(reconstruction, 0.0001));
        assert_eq!(
            gpu::SceneView::from_resolved(view).view_projection_matrix(),
            raster
        );
    }
    let governed = gpu::SceneView::new(
        gpu::CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        gpu::Viewport {
            width: viewport.width,
            height: viewport.height,
        },
    )
    .unwrap();
    assert!(governed.resolved_view().contains_point(Vec3::ZERO));
}

#[cfg(feature = "scene-viewer")]
#[test]
fn facade_orbit_motion_and_resize_keep_shared_screen_queries_on_target() {
    use worldrunner::scene::viewer::{OrbitCamera, OrbitLimits};

    let mut orbit = OrbitCamera::new(
        gpu::CameraPose {
            eye_millimeters: [2000, -2000, 2000],
            target_millimeters: [0, 0, 0],
        },
        OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 10_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
    )
    .unwrap();
    for (width, height) in [(800, 600), (1600, 1200)] {
        let viewport = gpu::Viewport { width, height };
        orbit.drag([80.0, 20.0], viewport);
        orbit.zoom(1.0);
        let view = orbit
            .view(
                plan::SceneCamera {
                    vertical_fov_millidegrees: 60_000,
                    near_plane_millimeters: 10,
                    far_plane_millimeters: 100_000,
                },
                viewport,
            )
            .unwrap();
        let resolved = view.resolved_view();
        let center = resolved.world_to_screen(Vec3::ZERO).unwrap();
        assert!((f64::from(center.x) - f64::from(width) * 0.5).abs() < 0.001);
        assert!((f64::from(center.y) - f64::from(height) * 0.5).abs() < 0.001);
        assert!(resolved.screen_to_world(center).unwrap().length() < 0.001);
        let ray = resolved.screen_ray(center.truncate()).unwrap();
        let hit = ray
            .intersect_plane(Vec3::ZERO, resolved.eye_meters())
            .unwrap();
        assert!(hit.length() < 0.001);
    }
}
