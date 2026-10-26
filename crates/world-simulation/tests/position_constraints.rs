#[test]
fn constrained_off_center_contact_retains_angular_response() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, handle) = body(
        &mut world,
        Vec2::ZERO,
        Body2d {
            freeze_position: 1,
            freeze_rotation: false,
            gravity_scale: 0.0,
            ..Default::default()
        },
    );
    body(
        &mut world,
        Vec2::new(0.9, 0.5),
        Body2d {
            body_type: worldrunner_world_simulation::physics2d::BodyType::Static,
            ..Default::default()
        },
    );
    world.step_physics(0.1).unwrap();
    let transform = world.local_transform(object).unwrap();
    assert_eq!(transform.translation.x.to_bits(), 0.0_f32.to_bits());
    assert!(transform.rotation.z.abs() > 1e-4);
    assert!(matches!(
        world.component(handle).unwrap().desc,
        ComponentDesc::Body2d(_)
    ));
    assert!(!world.contacts().is_empty());
}

#[test]
fn blocked_contact_normals_allow_tangent_motion_and_deliver_contacts() {
    for (mask, normal, tangent) in [(1, Vec2::X, Vec2::Y), (2, Vec2::Y, Vec2::X)] {
        for rotation_frozen in [false, true] {
            let mut world = World::new(WorldSettings::default()).unwrap();
            let (object, _) = body(
                &mut world,
                Vec2::ZERO,
                Body2d {
                    freeze_position: mask,
                    freeze_rotation: rotation_frozen,
                    velocity: tangent * 0.1,
                    gravity_scale: 0.0,
                    ..Default::default()
                },
            );
            body(
                &mut world,
                normal * 0.9,
                Body2d {
                    body_type: worldrunner_world_simulation::physics2d::BodyType::Static,
                    ..Default::default()
                },
            );
            world.step_physics(0.1).unwrap();
            let position = world
                .local_transform(object)
                .unwrap()
                .translation
                .truncate();
            assert_eq!(position.dot(normal).to_bits(), 0.0_f32.to_bits());
            assert!((position.dot(tangent) - 0.01).abs() < 1e-6);
            assert!(!world.contacts().is_empty());
            assert!(world.next_contact().is_some());
        }
    }
}

#[test]
fn both_bodies_frozen_on_contact_axis_advance_without_fault() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for position in [Vec2::ZERO, Vec2::new(0.9, 0.0)] {
        body(
            &mut world,
            position,
            Body2d {
                freeze_position: 1,
                freeze_rotation: true,
                velocity: Vec2::Y * 0.1,
                gravity_scale: 0.0,
                ..Default::default()
            },
        );
    }
    assert!(matches!(
        world
            .advance(worldrunner_world_simulation::TimeSpan::new(1, 10).unwrap())
            .unwrap(),
        worldrunner_world_simulation::Advance::Complete(_)
    ));
    assert!(!world.contacts().is_empty());
}

use glam::{Vec2, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, LocalTransform, ObjectDesc, ObjectHandle, World, WorldSettings,
    physics2d::{Body2d, Collider2d, Shape2d},
};

fn body(world: &mut World, position: Vec2, value: Body2d) -> (ObjectHandle, ComponentHandle) {
    let object = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: position.extend(0.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    let handle = world
        .add_component(object, ComponentDesc::Body2d(value))
        .unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Collider2d(Collider2d {
                shapes: vec![Shape2d::Box {
                    half_extents: Vec2::splat(0.5),
                }],
                ..Default::default()
            }),
        )
        .unwrap();
    (object, handle)
}

#[test]
fn heavy_paddle_keeps_exact_x_during_fast_collision_and_move_position() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (paddle, handle) = body(
        &mut world,
        Vec2::ZERO,
        Body2d {
            mass: 1e6,
            freeze_position: 1,
            freeze_rotation: true,
            gravity_scale: 0.0,
            ..Default::default()
        },
    );
    let (ball, _) = body(
        &mut world,
        Vec2::new(-5.0, 0.0),
        Body2d {
            velocity: Vec2::new(100.0, 0.0),
            mass: 0.0001,
            gravity_scale: 0.0,
            restitution: 1.0,
            ..Default::default()
        },
    );
    world
        .move_body_position(handle, Vec2::new(10.0, 0.5))
        .unwrap();
    world.step_physics(0.1).unwrap();
    let position = world.local_transform(paddle).unwrap().translation;
    assert_eq!(position.x.to_bits(), 0.0_f32.to_bits());
    assert!((position.y - 0.5).abs() < 1e-5);
    assert!(world.local_transform(ball).unwrap().translation.x < 0.0);
    let ComponentDesc::Body2d(value) = &world.component(handle).unwrap().desc else {
        panic!()
    };
    assert_eq!(value.velocity.x.to_bits(), 0.0_f32.to_bits());
    assert!((value.velocity.y - 5.0).abs() < 1e-5);
    assert_eq!(value.angular_velocity.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn frozen_y_blocks_gravity_target_and_ccd_impulses() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (paddle, handle) = body(
        &mut world,
        Vec2::ZERO,
        Body2d {
            mass: 1e6,
            freeze_position: 2,
            freeze_rotation: true,
            ..Default::default()
        },
    );
    body(
        &mut world,
        Vec2::new(0.0, -5.0),
        Body2d {
            velocity: Vec2::new(0.0, 100.0),
            gravity_scale: 0.0,
            ..Default::default()
        },
    );
    world
        .move_body_position(handle, Vec2::new(0.5, 100.0))
        .unwrap();
    world.step_physics(0.1).unwrap();
    let position = world.local_transform(paddle).unwrap().translation;
    assert_eq!(position.y.to_bits(), 0.0_f32.to_bits());
    assert!((position.x - 0.5).abs() < 1e-5);
    world.step_physics(0.1).unwrap();
    assert_eq!(
        world
            .local_transform(paddle)
            .unwrap()
            .translation
            .y
            .to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn frozen_velocity_does_not_sweep_into_a_distant_wall() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (object, _) = body(
        &mut world,
        Vec2::ZERO,
        Body2d {
            freeze_position: 3,
            velocity: Vec2::splat(1000.0),
            ..Default::default()
        },
    );
    let (wall, _) = body(
        &mut world,
        Vec2::new(5.0, 0.0),
        Body2d {
            body_type: worldrunner_world_simulation::physics2d::BodyType::Static,
            ..Default::default()
        },
    );
    world.step_physics(0.1).unwrap();
    assert_eq!(
        world.local_transform(object).unwrap().translation,
        Vec3::ZERO
    );
    assert_eq!(
        world.local_transform(wall).unwrap().translation.x.to_bits(),
        5.0_f32.to_bits()
    );
    assert!(world.contacts().is_empty());
}
