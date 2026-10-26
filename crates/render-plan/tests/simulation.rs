#![cfg(feature = "simulation")]
use glam::{Mat4, Vec3};
use worldrunner_render_plan::simulation::{SpriteTextures, extract_world};
use worldrunner_world_simulation::presentation::{Camera, Sprite, SpriteRenderer, Tile, Tilemap};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}
#[test]
fn sprites_keep_texture_tint_uv_flip_and_stable_sorting() {
    let mut w = world();
    let object = w.create_object(ObjectDesc::default()).unwrap();
    let sprite = Sprite {
        identity: 0,
        texture: 7,
        rect: [0., 0., 2., 4.],
        pivot: [0.5, 0.5],
        pixels_per_unit: 2.,
        texture_size: [8, 8],
    };
    w.add_component(
        object,
        ComponentDesc::Sprite(SpriteRenderer {
            sprite: Some(sprite),
            color: [0.2, 0.4, 0.6, 0.5],
            flip_x: true,
            sorting_order: 2,
            ..Default::default()
        }),
    )
    .unwrap();
    let plan = extract_world(&w, &SpriteTextures::default(), [80, 40]).unwrap();
    assert_eq!(plan.quads.len(), 1);
    assert_eq!(plan.quads[0].texture, Some(7));
    assert_eq!(
        plan.quads[0].uv_rect.map(f32::to_bits),
        [0.25, 0.5, 0., 1.].map(f32::to_bits)
    );
    assert_eq!(
        plan.quads[0].color.map(f32::to_bits),
        [0.2, 0.4, 0.6, 0.5].map(f32::to_bits)
    );
    assert!(Vec3::from_array(plan.quads[0].corners[0]).abs_diff_eq(Vec3::new(-0.5, 0., -1.), 1e-6));
}
#[test]
fn tile_matrix_and_object_hierarchy_are_composed_without_decomposition() {
    let mut w = world();
    let parent = scaled_parent(&mut w);
    let object = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
                translation: Vec3::new(2., 3., 4.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    let sprite = Sprite {
        identity: 0,
        texture: 0,
        rect: [0., 0., 2., 2.],
        pivot: [0., 0.],
        pixels_per_unit: 1.,
        texture_size: [2, 2],
    };
    w.set_parent(
        object,
        Some(parent),
        worldrunner_world_simulation::ParentSpace::KeepLocal,
    )
    .unwrap();
    let shear = Mat4::from_cols_array(&[
        1., 0., 0., 0., 0.5, 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ]);
    w.add_component(
        object,
        ComponentDesc::Tilemap(Tilemap {
            tiles: vec![Tile {
                sprite,
                transform: shear,
                color: [1.; 4],
            }],
            ..Default::default()
        }),
    )
    .unwrap();
    let plan = extract_world(&w, &SpriteTextures::default(), [20, 20]).unwrap();
    let basis = Mat4::from_cols_array(&[
        1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.,
    ]);
    let matrix = basis * w.world_matrix(object).unwrap() * shear;
    for (actual, local) in
        plan.quads[0]
            .corners
            .iter()
            .zip([[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [0., 2., 0.]])
    {
        assert!(
            Vec3::from_array(*actual)
                .abs_diff_eq(matrix.transform_point3(Vec3::from_array(local)), 1e-6)
        );
    }
}
#[test]
fn camera_clear_alpha_and_queries_use_the_render_camera() {
    let mut w = world();
    let camera = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    w.add_component(
        camera,
        ComponentDesc::Camera(Camera {
            background: [0.1, 0.2, 0.3, 0.25],
            ..Default::default()
        }),
    )
    .unwrap();
    let plan = extract_world(&w, &SpriteTextures::default(), [80, 40]).unwrap();
    assert_eq!(
        plan.clear.map(f64::to_bits),
        [0.1, 0.2, 0.3, 0.25].map(f64::to_bits)
    );
    let view = plan.camera.unwrap();
    let screen = view.world_to_screen(Vec3::ZERO).unwrap();
    assert!((screen.x - 40.).abs() < 0.001 && (screen.y - 20.).abs() < 0.001);
    assert!(view.screen_to_world(screen).unwrap().length() < 0.001);
}

fn scaled_parent(w: &mut World) -> worldrunner_world_simulation::ObjectHandle {
    w.create_object(ObjectDesc {
        local_transform: LocalTransform {
            scale: Vec3::new(2., 3., 1.),
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap()
}
