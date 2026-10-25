use glam::{DVec3, Vec2, Vec3};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, ViewError, ViewHandedness,
    Viewport, resolve,
};

const CONVENTIONS: [ViewHandedness; 2] = [
    ViewHandedness::RightHanded,
    ViewHandedness::DirectXLeftHanded,
];

fn view(lens: CameraLens, handedness: ViewHandedness) -> ResolvedView {
    resolve(&CameraState {
        pose: CameraPose {
            eye_meters: DVec3::ZERO,
            forward: DVec3::NEG_Z,
            up: DVec3::Y,
        },
        lens,
        viewport: Viewport {
            width: 200,
            height: 100,
        },
        convention: ViewConvention { handedness },
    })
    .unwrap()
}

#[test]
fn orthographic_rays_remain_unit_length_across_representable_clip_scales() {
    for far_meters in [1.0e-25, 1.0e20] {
        for handedness in CONVENTIONS {
            let view = view(
                CameraLens::Orthographic {
                    half_height_meters: 1.0,
                    near_meters: 0.0,
                    far_meters,
                },
                handedness,
            );
            let ray = view.screen_ray(Vec2::new(100.0, 50.0)).unwrap();
            assert!(ray.origin_meters.abs_diff_eq(Vec3::ZERO, f32::EPSILON));
            assert!(ray.direction.abs_diff_eq(Vec3::NEG_Z, f32::EPSILON));
        }
    }
}

#[test]
fn corner_rays_follow_near_far_planes_and_top_left_pixels_for_both_lenses() {
    let lenses = [
        CameraLens::Perspective {
            vertical_fov_radians: std::f64::consts::FRAC_PI_2,
            near_meters: 1.0,
            far_meters: 10.0,
        },
        CameraLens::Orthographic {
            half_height_meters: 1.0,
            near_meters: 1.0,
            far_meters: 10.0,
        },
    ];
    for lens in lenses {
        for handedness in CONVENTIONS {
            assert_corner_rays(&view(lens, handedness), handedness);
        }
    }
}

fn assert_corner_rays(view: &ResolvedView, handedness: ViewHandedness) {
    let sign = if handedness == ViewHandedness::RightHanded {
        1.0
    } else {
        -1.0
    };
    for (x, y) in [(0.0, 0.0), (200.0, 0.0), (0.0, 100.0), (200.0, 100.0)] {
        let pixel = Vec2::new(x, y);
        let near = view.screen_to_world(pixel.extend(0.0)).unwrap();
        let far = view.screen_to_world(pixel.extend(1.0)).unwrap();
        assert!((near.z + 1.0).abs() < 0.00001);
        assert!((far.z + 10.0).abs() < 0.0001);
        assert!((near.x - sign * (x / 50.0 - 2.0)).abs() < 0.00001);
        assert!((near.y - (1.0 - y / 50.0)).abs() < 0.00001);
        let ray = view.screen_ray(pixel).unwrap();
        assert_eq!(ray.origin_meters, near);
        assert!((ray.direction.length() - 1.0).abs() < 0.00001);
        let hit = ray.intersect_plane(Vec3::NEG_Z * 10.0, Vec3::Z).unwrap();
        assert!(hit.abs_diff_eq(far, 0.0001));
        assert!(
            view.world_to_screen(hit)
                .unwrap()
                .abs_diff_eq(pixel.extend(1.0), 0.0001)
        );
    }
}

#[test]
fn frustum_rejects_each_clip_plane_and_accepts_interior_for_both_conventions() {
    for handedness in CONVENTIONS {
        let view = view(
            CameraLens::Orthographic {
                half_height_meters: 1.0,
                near_meters: 1.0,
                far_meters: 10.0,
            },
            handedness,
        );
        assert!(view.contains_point(Vec3::new(0.0, 0.0, -5.0)));
        for point in [
            Vec3::new(-2.1, 0.0, -5.0),
            Vec3::new(2.1, 0.0, -5.0),
            Vec3::new(0.0, -1.1, -5.0),
            Vec3::new(0.0, 1.1, -5.0),
            Vec3::new(0.0, 0.0, -0.9),
            Vec3::new(0.0, 0.0, -10.1),
        ] {
            assert!(!view.contains_point(point), "{handedness:?}: {point:?}");
        }
        let ray = view.screen_ray(Vec2::new(100.0, 50.0)).unwrap();
        assert_eq!(
            ray.intersect_plane(Vec3::ZERO, Vec3::Z),
            Err(ViewError::IntersectionBehindRay)
        );
    }
}

#[test]
fn ray_plane_intersection_is_independent_of_finite_normal_scale() {
    let view = view(
        CameraLens::Orthographic {
            half_height_meters: 1.0,
            near_meters: 0.0,
            far_meters: 10.0,
        },
        ViewHandedness::RightHanded,
    );
    let ray = view.screen_ray(Vec2::new(100.0, 50.0)).unwrap();
    let plane = Vec3::new(0.0, 0.0, -5.0);
    for scale in [1.0, f32::MAX, f32::MIN_POSITIVE, -f32::MAX] {
        let hit = ray.intersect_plane(plane, Vec3::Z * scale).unwrap();
        assert!(hit.abs_diff_eq(plane, f32::EPSILON));
    }
    assert_eq!(
        ray.intersect_plane(plane, Vec3::ZERO),
        Err(ViewError::ParallelRay)
    );
    assert_eq!(
        ray.intersect_plane(plane, Vec3::splat(f32::NAN)),
        Err(ViewError::InvalidWorldPoint)
    );
}
