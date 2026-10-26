use glam::{Quat, Vec2, Vec3};
use worldrunner_world_simulation::tilemap::{Tile, TileCell, TileCollider, TileLayout, Tilemap};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, LocalTransform, ObjectDesc, ParentSpace, World, WorldError,
    WorldSettings,
    physics2d::{Body2d, BodyType, Collider2d, ColliderGeometry, QueryFilter, Shape2d},
};

#[test]
fn independent_frozen_body_retains_world_orientation_under_rotating_parent() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let parent = w.create_object(ObjectDesc::default()).unwrap();
    let child = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::X,
                rotation: Quat::from_rotation_z(0.3),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    w.set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    w.add_component(
        parent,
        ComponentDesc::Body2d(Body2d {
            body_type: BodyType::Kinematic,
            angular_velocity: 1.0,
            ..Default::default()
        }),
    )
    .unwrap();
    w.add_component(
        child,
        ComponentDesc::Body2d(Body2d {
            body_type: BodyType::Kinematic,
            freeze_rotation: true,
            ..Default::default()
        }),
    )
    .unwrap();
    let before = w.world_matrix(child).unwrap();
    w.step_physics(0.1).unwrap();
    assert!(w.world_matrix(child).unwrap().abs_diff_eq(before, 1e-5));
}

#[test]
fn rejects_nonconvex_shapes_and_duplicate_bodies_atomically() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let o = w.create_object(ObjectDesc::default()).unwrap();
    let invalid = [
        vec![Vec2::ZERO, Vec2::ONE, Vec2::Y, Vec2::X],
        vec![Vec2::ZERO, Vec2::X, Vec2::new(0.2, 0.2), Vec2::Y],
        vec![Vec2::ZERO, Vec2::X, Vec2::X],
    ];
    for points in invalid {
        let before = w.snapshot();
        assert!(
            w.add_component(
                o,
                ComponentDesc::Collider2d(Collider2d {
                    geometry: ColliderGeometry::Shapes(vec![Shape2d::Polygon(points)]),
                    ..Default::default()
                })
            )
            .is_err()
        );
        assert_eq!(w.snapshot(), before);
    }
    w.add_component(o, ComponentDesc::Body2d(Body2d::default()))
        .unwrap();
    assert_eq!(
        w.add_component(o, ComponentDesc::Body2d(Body2d::default())),
        Err(WorldError::InvalidComponent(ComponentError::Duplicate))
    );
}

#[test]
fn rotated_scaled_tile_grid_and_overlap_shape_use_world_affine_matrix() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let o = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(4.0, 2.0, 0.0),
                rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                scale: Vec3::new(2.0, 1.0, 1.0),
            },
            ..Default::default()
        })
        .unwrap();
    let tilemap = Tilemap {
        layout: TileLayout {
            cell_gap: Vec2::new(0.5, 0.0),
            ..Default::default()
        },
        tiles: vec![Tile {
            collider: TileCollider::Grid,
            ..Default::default()
        }],
        cells: vec![TileCell {
            position: [-1, 0],
            tile: 0,
        }],
        ..Default::default()
    };
    w.add_component(o, ComponentDesc::Tilemap(tilemap)).unwrap();
    let collider = w
        .add_component(
            o,
            ComponentDesc::Collider2d(Collider2d {
                geometry: ColliderGeometry::Tilemap,
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(
        w.overlap_shape(
            &Shape2d::Circle { radius: 0.1 },
            Vec2::new(3.5, 0.0),
            QueryFilter::default()
        )
        .unwrap(),
        [collider]
    );
    assert!(
        w.overlap_point(Vec2::new(3.5, 1.8), QueryFilter::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn nonplanar_and_elliptical_circle_transforms_fail_without_advancing() {
    for transform in [
        LocalTransform {
            rotation: Quat::from_rotation_x(0.5),
            ..Default::default()
        },
        LocalTransform {
            scale: Vec3::new(2.0, 1.0, 1.0),
            ..Default::default()
        },
    ] {
        let mut w = World::new(WorldSettings::default()).unwrap();
        let o = w
            .create_object(ObjectDesc {
                local_transform: transform,
                ..Default::default()
            })
            .unwrap();
        w.add_component(o, ComponentDesc::Body2d(Body2d::default()))
            .unwrap();
        w.add_component(
            o,
            ComponentDesc::Collider2d(Collider2d {
                geometry: ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: 1.0 }]),
                ..Default::default()
            }),
        )
        .unwrap();
        let before = w.snapshot();
        assert!(w.step_physics(0.1).is_err());
        assert_eq!(w.snapshot(), before);
    }
}

#[test]
fn overflowed_static_geometry_returns_a_typed_error() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let o = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                scale: Vec3::splat(1e30),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    w.add_component(
        o,
        ComponentDesc::Collider2d(Collider2d {
            geometry: ColliderGeometry::Shapes(vec![Shape2d::Circle { radius: 1e30 }]),
            ..Default::default()
        }),
    )
    .unwrap();
    let before = w.snapshot();
    assert!(matches!(w.step_physics(0.1), Err(WorldError::Physics(_))));
    assert_eq!(w.snapshot(), before);
    assert!(w.overlap_point(Vec2::ZERO, QueryFilter::default()).is_err());
}
