//! Which camera a frame presents, and the lens it gets: the screen camera is the enabled camera
//! with the highest depth that draws to display 0 and not into a texture.
use worldrunner_camera::CameraLens;
use worldrunner_world_simulation::presentation::extraction::{
    SpriteTextures, WorldRenderPlan, extract_world,
};
use worldrunner_world_simulation::presentation::{Camera, CameraClear, Projection};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ObjectDesc, World, WorldError, WorldSettings,
};

fn world_of(cameras: &[Camera]) -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for camera in cameras {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(object, ComponentDesc::Camera(*camera))
            .unwrap();
    }
    world
}

fn plan(cameras: &[Camera]) -> Result<WorldRenderPlan, WorldError> {
    extract_world(&world_of(cameras), &SpriteTextures::default(), [64, 32])
}

fn lens_of(plan: &WorldRenderPlan) -> CameraLens {
    plan.camera_state.expect("a presented camera").lens
}

fn ortho(depth: i32, half_height: f32) -> Camera {
    Camera {
        depth,
        half_height,
        ..Default::default()
    }
}

fn perspective(depth: i32) -> Camera {
    Camera {
        projection: Projection::Perspective,
        depth,
        field_of_view: 90.,
        near: 0.5,
        far: 200.,
        ..Default::default()
    }
}

#[test]
fn a_camera_drawing_into_a_texture_is_not_the_screen_camera() {
    let into_texture = Camera {
        target_texture: 7,
        ..ortho(10, 9.)
    };
    let plan = plan(&[ortho(0, 3.), into_texture]).unwrap();
    assert!(matches!(
        lens_of(&plan),
        CameraLens::Orthographic { half_height_meters, .. } if half_height_meters == 3.
    ));
}

#[test]
fn a_camera_on_another_display_is_not_the_screen_camera() {
    let second_display = Camera {
        target_display: 1,
        ..ortho(10, 9.)
    };
    let plan = plan(&[second_display, ortho(0, 3.)]).unwrap();
    assert!(matches!(
        lens_of(&plan),
        CameraLens::Orthographic { half_height_meters, .. } if half_height_meters == 3.
    ));
}

#[test]
fn without_a_screen_camera_the_frame_has_no_camera() {
    let into_texture = Camera {
        target_texture: 7,
        ..Default::default()
    };
    let second_display = Camera {
        target_display: 2,
        ..Default::default()
    };
    let plan = plan(&[into_texture, second_display]).unwrap();
    assert!(plan.camera.is_none() && plan.camera_state.is_none());
}

#[test]
fn a_disabled_screen_camera_is_skipped() {
    let off = Camera {
        enabled: false,
        ..ortho(10, 9.)
    };
    let plan = plan(&[off, perspective(0)]).unwrap();
    assert!(matches!(lens_of(&plan), CameraLens::Perspective { .. }));
}

#[test]
fn the_highest_depth_screen_camera_wins_whatever_its_projection() {
    let high = plan(&[ortho(0, 3.), perspective(5)]).unwrap();
    assert!(matches!(lens_of(&high), CameraLens::Perspective { .. }));
    let low = plan(&[perspective(-5), ortho(0, 3.)]).unwrap();
    assert!(matches!(lens_of(&low), CameraLens::Orthographic { .. }));
}

#[test]
fn a_perspective_camera_presents_with_its_field_of_view_in_radians() {
    let plan = plan(&[perspective(0)]).unwrap();
    let CameraLens::Perspective {
        vertical_fov_radians,
        near_meters,
        far_meters,
    } = lens_of(&plan)
    else {
        panic!("expected a perspective lens");
    };
    assert!((vertical_fov_radians - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!((near_meters, far_meters), (0.5, 200.));
    let view = plan.camera.expect("a resolved view");
    assert_eq!((view.viewport().width, view.viewport().height), (64, 32));
}

#[test]
fn a_perspective_camera_with_no_near_plane_cannot_be_presented() {
    let flat = Camera {
        near: 0.,
        ..perspective(0)
    };
    assert_eq!(
        plan(&[flat]),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("nearClipPlane 0 with perspective projection")
        ))
    );
}

#[test]
fn a_perspective_camera_still_rejects_a_replaced_projection_matrix() {
    let replaced = Camera {
        projection_matrix: Some([
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ]),
        ..perspective(0)
    };
    assert_eq!(
        plan(&[replaced]),
        Err(WorldError::InvalidComponent(
            ComponentError::UnsupportedField("projectionMatrix")
        ))
    );
}

fn overlay(depth: i32, clear: CameraClear) -> Camera {
    Camera {
        clear,
        ..ortho(depth, 9.)
    }
}

#[test]
fn an_overlay_camera_above_a_clearing_camera_does_not_replace_it() {
    for clear in [CameraClear::DepthOnly, CameraClear::Nothing] {
        let plan = plan(&[perspective(0), overlay(10, clear)]).unwrap();
        assert!(
            matches!(lens_of(&plan), CameraLens::Perspective { .. }),
            "{clear:?}"
        );
    }
}

#[test]
fn the_highest_clearing_camera_is_the_base_below_several_overlays() {
    let base = Camera {
        clear: CameraClear::Skybox,
        ..ortho(5, 3.)
    };
    let plan = plan(&[
        ortho(1, 7.),
        overlay(10, CameraClear::Nothing),
        base,
        overlay(11, CameraClear::DepthOnly),
    ])
    .unwrap();
    assert!(matches!(
        lens_of(&plan),
        CameraLens::Orthographic { half_height_meters, .. } if half_height_meters == 3.
    ));
}

#[test]
fn an_overlay_camera_with_no_camera_below_it_is_the_base_and_clears_to_its_background() {
    for clear in [CameraClear::DepthOnly, CameraClear::Nothing] {
        let lone = Camera {
            background: [0.25, 0.5, 0.75, 1.],
            ..overlay(0, clear)
        };
        let plan = plan(&[lone]).unwrap();
        assert!(plan.camera_state.is_some(), "{clear:?}");
        assert_eq!(plan.clear, [0.25, 0.5, 0.75, 1.], "{clear:?}");
    }
}

#[test]
fn a_clearing_camera_into_a_texture_is_no_base_for_an_overlay() {
    let into_texture = Camera {
        target_texture: 4,
        background: [1., 0., 0., 1.],
        ..ortho(0, 3.)
    };
    let top = Camera {
        background: [0., 1., 0., 1.],
        ..overlay(10, CameraClear::Nothing)
    };
    assert_eq!(plan(&[into_texture, top]).unwrap().clear, [0., 1., 0., 1.]);
}
