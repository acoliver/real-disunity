use glam::{DVec3, Vec2, Vec3};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, DepthConvention, ViewConvention, ViewError,
    ViewHandedness, Viewport, resolve,
};

fn state(lens: CameraLens, handedness: ViewHandedness) -> CameraState {
    CameraState {
        pose: CameraPose {
            eye_meters: DVec3::new(3.0, -10.0, 2.0),
            forward: DVec3::Y,
            up: DVec3::Z,
        },
        lens,
        viewport: Viewport {
            width: 200,
            height: 100,
        },
        convention: ViewConvention { handedness },
    }
}

fn perspective() -> CameraLens {
    CameraLens::Perspective {
        vertical_fov_radians: std::f64::consts::FRAC_PI_2,
        near_meters: 1.0,
        far_meters: 21.0,
    }
}

fn orthographic() -> CameraLens {
    CameraLens::Orthographic {
        half_height_meters: 2.0,
        near_meters: 0.0,
        far_meters: 20.0,
    }
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.0001),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn projection_inverse_and_reconstruction_share_raster_landmarks() {
    for lens in [perspective(), orthographic()] {
        for handedness in [
            ViewHandedness::RightHanded,
            ViewHandedness::DirectXLeftHanded,
        ] {
            let view = resolve(&state(lens, handedness)).unwrap();
            assert_eq!(view.depth_convention(), DepthConvention::ZeroToOne);
            close(view.eye_meters(), Vec3::new(3.0, -10.0, 2.0));
            for point in [Vec3::new(3.0, -5.0, 2.0), Vec3::new(4.0, 0.0, 3.0)] {
                let screen = view.world_to_screen(point).unwrap();
                close(view.screen_to_world(screen).unwrap(), point);
                let ndc = view.view_projection_matrix().project_point3(point);
                close(
                    view.inverse_view_projection_matrix().project_point3(ndc),
                    point,
                );
                let reconstructed = view
                    .reconstruction_projection_matrix()
                    .project_point3(view.reconstruction_view_matrix().transform_point3(point));
                close(reconstructed, ndc);
                assert!(view.contains_point(point));
            }
            let screen = view.world_to_screen(Vec3::new(4.0, 0.0, 3.0)).unwrap();
            assert!(screen.y < 50.0, "canonical +Z is above screen centre");
            assert_eq!(screen.x > 100.0, handedness == ViewHandedness::RightHanded);
        }
    }
}

#[test]
fn near_far_and_frustum_use_non_reversed_zero_to_one_depth() {
    for lens in [perspective(), orthographic()] {
        let view = resolve(&state(lens, ViewHandedness::RightHanded)).unwrap();
        let (near, far) = match lens {
            CameraLens::Perspective {
                near_meters,
                far_meters,
                ..
            }
            | CameraLens::Orthographic {
                near_meters,
                far_meters,
                ..
            } => (near_meters, far_meters),
        };
        for (distance, depth) in [(near, 0.0), (far, 1.0)] {
            let point = (DVec3::new(3.0, -10.0, 2.0) + DVec3::Y * distance).as_vec3();
            assert!((view.world_to_screen(point).unwrap().z - depth).abs() < 0.00001);
        }
        assert!(!view.contains_point(Vec3::new(3.0, -11.0, 2.0)));
        assert!(!view.contains_point(Vec3::new(3.0, 12.0, 2.0)));
        assert!(!view.contains_point(Vec3::new(1000.0, 0.0, 2.0)));
        assert!(!view.contains_point(Vec3::splat(f32::NAN)));
    }
}

#[test]
fn orthographic_half_height_and_resize_preserve_vertical_framing() {
    let mut camera = state(orthographic(), ViewHandedness::RightHanded);
    let original = resolve(&camera).unwrap();
    close(
        original.world_to_screen(Vec3::new(5.0, 0.0, 3.0)).unwrap(),
        Vec3::new(150.0, 25.0, 0.5),
    );
    close(
        original.world_to_screen(Vec3::new(5.0, 5.0, 3.0)).unwrap(),
        Vec3::new(150.0, 25.0, 0.75),
    );
    camera.viewport.width = 400;
    let resized = resolve(&camera).unwrap();
    close(
        resized.world_to_screen(Vec3::new(5.0, 0.0, 3.0)).unwrap(),
        Vec3::new(250.0, 25.0, 0.5),
    );
    assert_eq!(resized.viewport(), camera.viewport);
}

#[test]
fn explicit_planar_basis_supports_rays_and_plane_queries() {
    for lens in [perspective(), orthographic()] {
        let mut camera = state(lens, ViewHandedness::RightHanded);
        camera.pose = CameraPose {
            eye_meters: DVec3::new(0.0, 0.0, 10.0),
            forward: DVec3::NEG_Z,
            up: DVec3::Y,
        };
        let view = resolve(&camera).unwrap();
        let center = view.screen_ray(Vec2::new(100.0, 50.0)).unwrap();
        close(center.direction, Vec3::NEG_Z);
        close(
            center.intersect_plane(Vec3::ZERO, Vec3::Z).unwrap(),
            Vec3::ZERO,
        );
        let side = view.screen_ray(Vec2::new(150.0, 25.0)).unwrap();
        let hit = side.intersect_plane(Vec3::ZERO, Vec3::Z).unwrap();
        let screen = view.world_to_screen(hit).unwrap();
        assert!((screen.x - 150.0).abs() < 0.0001 && (screen.y - 25.0).abs() < 0.0001);
        if matches!(lens, CameraLens::Orthographic { .. }) {
            close(side.direction, center.direction);
            close(hit, Vec3::new(2.0, 1.0, 0.0));
        } else {
            assert!(side.direction.x > 0.0 && side.direction.y > 0.0);
        }
        assert_eq!(
            center.intersect_plane(Vec3::X, Vec3::X),
            Err(ViewError::ParallelRay)
        );
    }
}

#[test]
fn invalid_camera_inputs_are_diagnosed_without_nan_matrices() {
    let valid = state(perspective(), ViewHandedness::RightHanded);
    for viewport in [
        Viewport {
            width: 0,
            height: 1,
        },
        Viewport {
            width: 1,
            height: 0,
        },
    ] {
        assert_eq!(
            resolve(&CameraState { viewport, ..valid }),
            Err(ViewError::DegenerateViewport {
                width: viewport.width,
                height: viewport.height
            })
        );
    }
    for eye in [DVec3::splat(f64::NAN), DVec3::splat(f64::INFINITY)] {
        assert_eq!(
            resolve(&CameraState {
                pose: CameraPose {
                    eye_meters: eye,
                    ..valid.pose
                },
                ..valid
            }),
            Err(ViewError::NonFinitePose)
        );
    }
    assert_eq!(
        resolve(&CameraState {
            pose: CameraPose {
                forward: DVec3::ZERO,
                ..valid.pose
            },
            ..valid
        }),
        Err(ViewError::DegenerateDirection)
    );
    for up in [DVec3::ZERO, DVec3::Y] {
        assert_eq!(
            resolve(&CameraState {
                pose: CameraPose { up, ..valid.pose },
                ..valid
            }),
            Err(ViewError::DegenerateRoll)
        );
    }
}

#[test]
fn invalid_lenses_are_diagnosed_without_nan_matrices() {
    let valid = state(perspective(), ViewHandedness::RightHanded);
    for lens in [
        CameraLens::Perspective {
            vertical_fov_radians: 0.0,
            near_meters: 1.0,
            far_meters: 10.0,
        },
        CameraLens::Perspective {
            vertical_fov_radians: std::f64::consts::PI,
            near_meters: 1.0,
            far_meters: 10.0,
        },
        CameraLens::Perspective {
            vertical_fov_radians: 1.0,
            near_meters: 0.0,
            far_meters: 10.0,
        },
        CameraLens::Perspective {
            vertical_fov_radians: 1.0,
            near_meters: 10.0,
            far_meters: 10.0,
        },
    ] {
        assert_eq!(
            resolve(&CameraState { lens, ..valid }),
            Err(ViewError::InvalidPerspectiveLens)
        );
    }
    for half_height_meters in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let lens = CameraLens::Orthographic {
            half_height_meters,
            near_meters: 0.0,
            far_meters: 10.0,
        };
        assert_eq!(
            resolve(&CameraState { lens, ..valid }),
            Err(ViewError::InvalidOrthographicLens)
        );
    }
}

#[test]
fn unrepresentable_transforms_and_invalid_queries_fail_explicitly() {
    let valid = state(perspective(), ViewHandedness::RightHanded);
    let lens = CameraLens::Orthographic {
        half_height_meters: f64::MAX,
        near_meters: 0.0,
        far_meters: 10.0,
    };
    assert_eq!(
        resolve(&CameraState { lens, ..valid }),
        Err(ViewError::SingularTransform)
    );
    let view = resolve(&valid).unwrap();
    assert_eq!(
        view.screen_to_world(Vec3::new(0.0, 0.0, -0.1)),
        Err(ViewError::InvalidScreenPoint)
    );
    assert_eq!(
        view.screen_ray(Vec2::splat(f32::NAN)),
        Err(ViewError::InvalidScreenPoint)
    );
    assert_eq!(
        view.world_to_screen(Vec3::splat(f32::NAN)),
        Err(ViewError::InvalidWorldPoint)
    );
    assert_eq!(
        view.world_to_screen(view.eye_meters()),
        Err(ViewError::PointAtInfinity)
    );
}

#[test]
fn finite_lenses_with_unrepresentable_far_plane_are_rejected() {
    let lens = CameraLens::Perspective {
        vertical_fov_radians: 1.0,
        near_meters: 0.1,
        far_meters: 1.0e20,
    };
    assert_eq!(
        resolve(&state(lens, ViewHandedness::RightHanded)),
        Err(ViewError::SingularTransform)
    );
}

#[test]
fn finite_world_points_that_overflow_screen_coordinates_are_diagnosed() {
    let view = resolve(&state(orthographic(), ViewHandedness::RightHanded)).unwrap();
    assert_eq!(
        view.world_to_screen(Vec3::new(f32::MAX, 0.0, 2.0)),
        Err(ViewError::PointAtInfinity)
    );
}
