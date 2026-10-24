use super::*;
use worldrunner_camera::{CameraLens, CameraState, ViewConvention, ViewHandedness, resolve};

#[test]
fn scene_adapters_share_raster_inverse_and_canonical_landmarks() {
    let pose = CameraPose {
        eye_millimeters: [3000, -10_000, 2000],
        target_millimeters: [3000, 0, 2000],
    };
    let viewport = Viewport {
        width: 200,
        height: 100,
    };
    let views = [
        SceneView::new(pose, camera(), viewport).unwrap(),
        SceneView::orthographic_with_up(
            pose,
            super::super::OrthographicLens {
                vertical_half_extent_meters: 2.0,
                near_meters: 0.0,
                far_meters: 20.0,
            },
            [0.0, 0.0, 1.0],
            viewport,
        )
        .unwrap(),
    ];
    for view in views {
        let resolved = view.resolved_view();
        assert_eq!(
            view.view_projection_matrix(),
            resolved.view_projection_matrix()
        );
        assert_eq!(view.projection_matrix(), resolved.projection_matrix());
        assert_eq!(view.view_matrix(), resolved.view_matrix());
        assert_eq!(view.eye_meters(), resolved.eye_meters());
        let point = Vec3::new(4.0, 0.0, 3.0);
        let screen = resolved.world_to_screen(point).unwrap();
        assert!(screen.x > 100.0 && screen.y < 50.0);
        assert!(
            resolved
                .screen_to_world(screen)
                .unwrap()
                .abs_diff_eq(point, 0.001)
        );
    }
}

#[test]
fn resolved_camera_can_enter_existing_scene_draw_contract_without_reresolving() {
    let resolved = resolve(&CameraState {
        pose: worldrunner_camera::CameraPose {
            eye_meters: glam::DVec3::new(0.0, 0.0, 10.0),
            forward: glam::DVec3::NEG_Z,
            up: glam::DVec3::Y,
        },
        lens: CameraLens::Orthographic {
            half_height_meters: 3.0,
            near_meters: 0.0,
            far_meters: 20.0,
        },
        viewport: worldrunner_camera::Viewport {
            width: 65,
            height: 47,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::DirectXLeftHanded,
        },
    })
    .unwrap();
    let view = SceneView::from_resolved(resolved);
    assert_eq!(*view.resolved_view(), resolved);
    assert_eq!(
        view.viewport(),
        Viewport {
            width: 65,
            height: 47
        }
    );
}
