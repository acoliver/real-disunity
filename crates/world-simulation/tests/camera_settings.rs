//! Camera settings beyond the orthographic lens: defaults, validation, what a frame can present,
//! and which objects a culling mask admits.
use worldrunner_world_simulation::presentation::extraction::{SpriteTextures, extract_world};
use worldrunner_world_simulation::presentation::{
    Camera, CameraClear, CameraFlags, Projection, Sprite, SpriteRenderer,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ObjectDesc, ObjectHandle, World, WorldError, WorldSettings,
};

fn world_with(camera: Camera) -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Camera(camera))
        .unwrap();
    (world, object)
}

fn sprite_on(world: &mut World, layer: u8) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world.set_layer(object, layer).unwrap();
    let sprite = Sprite {
        identity: 0,
        texture: 1,
        rect: [0., 0., 4., 4.],
        pivot: [0.5, 0.5],
        pixels_per_unit: 4.,
        texture_size: [4, 4],
    };
    let renderer = SpriteRenderer {
        sprite: Some(sprite),
        ..Default::default()
    };
    world
        .add_component(object, ComponentDesc::Sprite(renderer))
        .unwrap();
}

fn quads(world: &World) -> Result<usize, WorldError> {
    extract_world(world, &SpriteTextures::default(), [64, 64]).map(|plan| plan.quads.len())
}

fn unsupported(field: &'static str) -> WorldError {
    WorldError::InvalidComponent(ComponentError::UnsupportedField(field))
}

#[test]
fn a_new_camera_has_unitys_defaults_for_the_added_settings() {
    let camera = Camera::default();
    assert_eq!(camera.projection, Projection::Orthographic);
    assert_eq!(camera.field_of_view.to_bits(), 60f32.to_bits());
    assert_eq!(camera.culling_mask, u32::MAX);
    assert_eq!(
        camera.viewport.map(f32::to_bits),
        [0., 0., 1., 1.].map(f32::to_bits)
    );
    assert_eq!(camera.depth_texture_mode, 0);
    assert!(camera.flags.contains(CameraFlags::HDR));
    assert!(camera.flags.contains(CameraFlags::MSAA));
    assert!(camera.flags.contains(CameraFlags::OCCLUSION_CULLING));
    assert!(!camera.flags.contains(CameraFlags::DYNAMIC_RESOLUTION));
    assert_eq!((camera.target_display, camera.target_texture), (0, 0));
    assert_eq!(
        (camera.projection_matrix, camera.world_to_camera),
        (None, None)
    );
    assert_eq!(camera.camera_type, 1);
    assert_eq!(camera.validate(), Ok(()));
}

#[test]
fn settings_outside_unitys_ranges_are_invalid() {
    let invalid = [
        Camera {
            field_of_view: 0.,
            ..Default::default()
        },
        Camera {
            field_of_view: 180.,
            ..Default::default()
        },
        Camera {
            field_of_view: f32::NAN,
            ..Default::default()
        },
        Camera {
            viewport: [0., 0., f32::INFINITY, 1.],
            ..Default::default()
        },
        Camera {
            viewport: [0., 0., -0.5, 1.],
            ..Default::default()
        },
        Camera {
            projection_matrix: Some([f32::NAN; 16]),
            ..Default::default()
        },
        Camera {
            world_to_camera: Some([f32::INFINITY; 16]),
            ..Default::default()
        },
        Camera {
            camera_type: 0,
            ..Default::default()
        },
        Camera {
            camera_type: 32,
            ..Default::default()
        },
        Camera {
            depth_texture_mode: 8,
            ..Default::default()
        },
        Camera {
            target_display: 8,
            ..Default::default()
        },
    ];
    for camera in invalid {
        assert!(camera.validate().is_err(), "{camera:?}");
    }
    let zero_size = Camera {
        viewport: [0.5, 0.5, 0., 0.],
        ..Default::default()
    };
    assert_eq!(zero_size.validate(), Ok(()), "Unity allows an empty rect");
}

#[test]
fn a_frame_rejects_each_setting_it_cannot_show() {
    let identity = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let cases = [
        (
            Camera {
                projection: Projection::Perspective,
                ..Default::default()
            },
            "perspective projection",
        ),
        (
            Camera {
                projection_matrix: Some(identity),
                ..Default::default()
            },
            "projectionMatrix",
        ),
        (
            Camera {
                world_to_camera: Some(identity),
                ..Default::default()
            },
            "worldToCameraMatrix",
        ),
        (
            Camera {
                target_texture: 7,
                ..Default::default()
            },
            "targetTexture",
        ),
        (
            Camera {
                target_display: 1,
                ..Default::default()
            },
            "targetDisplay",
        ),
        (
            Camera {
                viewport: [0., 0., 0.5, 1.],
                ..Default::default()
            },
            "rect",
        ),
    ];
    for (camera, field) in cases {
        let (world, _) = world_with(camera);
        assert_eq!(quads(&world), Err(unsupported(field)), "{camera:?}");
    }
}

#[test]
fn settings_that_do_not_change_a_two_dimensional_frame_are_accepted() {
    let camera = Camera {
        field_of_view: 90.,
        depth_texture_mode: 1,
        flags: CameraFlags::DEFAULT
            .with(CameraFlags::HDR, false)
            .with(CameraFlags::DYNAMIC_RESOLUTION, true),
        clear: CameraClear::SolidColor,
        ..Default::default()
    };
    let (mut world, _) = world_with(camera);
    sprite_on(&mut world, 0);
    assert_eq!(quads(&world), Ok(1));
}

#[test]
fn only_the_selected_camera_is_checked_for_presentation() {
    let (mut world, _) = world_with(Camera {
        projection: Projection::Perspective,
        depth: -1,
        ..Default::default()
    });
    let top = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(top, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    assert_eq!(quads(&world), Ok(0));
}

#[test]
fn the_culling_mask_admits_sprites_by_their_layer() {
    let (mut world, _) = world_with(Camera {
        culling_mask: 1 << 5,
        ..Default::default()
    });
    sprite_on(&mut world, 5);
    sprite_on(&mut world, 0);
    sprite_on(&mut world, 31);
    assert_eq!(quads(&world), Ok(1), "only layer 5 is in the mask");
    let (mut everything, _) = world_with(Camera::default());
    sprite_on(&mut everything, 31);
    assert_eq!(
        quads(&everything),
        Ok(1),
        "the default mask admits layer 31"
    );
    let (mut nothing, _) = world_with(Camera {
        culling_mask: 0,
        ..Default::default()
    });
    sprite_on(&mut nothing, 0);
    assert_eq!(quads(&nothing), Ok(0));
}

#[test]
fn sprites_are_all_drawn_when_there_is_no_camera() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    sprite_on(&mut world, 9);
    assert_eq!(quads(&world), Ok(1));
}
