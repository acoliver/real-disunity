use super::*;

fn limits() -> OrbitLimits {
    OrbitLimits {
        min_distance_millimeters: 100.0,
        max_distance_millimeters: 10_000.0,
        min_elevation_radians: -1.4,
        max_elevation_radians: 1.4,
    }
}
fn pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [2000, -2000, 2000],
        target_millimeters: [0, 0, 0],
    }
}
fn viewport() -> Viewport {
    Viewport {
        width: 800,
        height: 600,
    }
}
fn intrinsics() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 10,
        far_plane_millimeters: 100_000,
    }
}
#[test]
fn drag_changes_eye_but_not_target_and_reset_is_exact() {
    let mut camera = OrbitCamera::new(pose(), limits()).unwrap();
    camera.drag([100.0, 50.0], viewport());
    assert_ne!(camera.pose().eye_millimeters, pose().eye_millimeters);
    assert_eq!(camera.pose().target_millimeters, pose().target_millimeters);
    assert!(camera.view(intrinsics(), viewport()).is_ok());
    camera.zoom(4.0);
    camera.reset();
    assert_eq!(camera.pose(), pose());
}
#[test]
fn zoom_and_elevation_are_bounded_at_both_extremes() {
    let mut camera = OrbitCamera::new(pose(), limits()).unwrap();
    for (wheel, distance) in [(1.0e300, 100.0), (-1.0e300, 10_000.0)] {
        camera.zoom(wheel);
        let delta = vector(camera.pose().eye_millimeters);
        assert!((delta.length() - distance).abs() < 1.0);
    }
    for (drag, expected) in [(1.0e300, 1.4), (-1.0e300, -1.4)] {
        camera.drag([0.0, drag], viewport());
        let delta = vector(camera.pose().eye_millimeters);
        assert!(((delta.z / delta.length()).asin() - expected).abs() < 0.001);
        assert!(camera.view(intrinsics(), viewport()).is_ok());
    }
}
#[test]
fn normalized_drag_is_resolution_independent() {
    let mut a = OrbitCamera::new(pose(), limits()).unwrap();
    let mut b = a.clone();
    a.drag([80.0, 60.0], viewport());
    b.drag(
        [160.0, 120.0],
        Viewport {
            width: 1600,
            height: 1200,
        },
    );
    assert_eq!(a.pose(), b.pose());
}
#[test]
fn zero_size_and_nonfinite_inputs_do_not_change_camera() {
    let mut camera = OrbitCamera::new(pose(), limits()).unwrap();
    camera.drag(
        [100.0, 50.0],
        Viewport {
            width: 0,
            height: 100,
        },
    );
    camera.drag(
        [100.0, 50.0],
        Viewport {
            width: 100,
            height: 0,
        },
    );
    camera.drag([f64::NAN, 1.0], viewport());
    camera.zoom(f64::INFINITY);
    assert_eq!(camera.pose(), pose());
    assert!(matches!(
        camera.view(
            intrinsics(),
            Viewport {
                width: 0,
                height: 100
            }
        ),
        Err(ViewerError::View(_))
    ));
}
#[test]
fn invalid_bounds_and_initial_poses_fail_without_display() {
    for bad in [
        OrbitLimits {
            min_distance_millimeters: 0.0,
            ..limits()
        },
        OrbitLimits {
            max_distance_millimeters: f64::NAN,
            ..limits()
        },
        OrbitLimits {
            min_elevation_radians: -1.57,
            ..limits()
        },
        OrbitLimits {
            max_elevation_radians: 1.57,
            ..limits()
        },
        OrbitLimits {
            min_elevation_radians: 1.0,
            max_elevation_radians: -1.0,
            ..limits()
        },
    ] {
        assert!(OrbitCamera::new(pose(), bad).is_err());
    }
    assert!(
        OrbitCamera::new(
            CameraPose {
                eye_millimeters: [0; 3],
                target_millimeters: [0; 3]
            },
            limits()
        )
        .is_err()
    );
    assert!(
        OrbitCamera::new(
            CameraPose {
                eye_millimeters: [0, 0, 1000],
                target_millimeters: [0; 3]
            },
            limits()
        )
        .is_err()
    );
    assert!(
        OrbitCamera::new(
            pose(),
            OrbitLimits {
                max_distance_millimeters: 1000.0,
                ..limits()
            }
        )
        .is_err()
    );
}
#[test]
fn repeated_bounded_motion_stays_renderable() {
    let mut camera = OrbitCamera::new(pose(), limits()).unwrap();
    for i in 0..1000 {
        camera.drag([f64::from(i) - 500.0, f64::from(i % 7) - 3.0], viewport());
        camera.zoom(f64::from(i % 5) - 2.0);
        assert!(camera.view(intrinsics(), viewport()).is_ok());
    }
}
