use glam::{Vec2, Vec3};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
    physics2d::{Body2d, BodyType, Collider2d},
};

#[test]
fn movement_target_uses_continuous_collision_instead_of_teleporting() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let moving = world.create_object(ObjectDesc::default()).unwrap();
    let body = world
        .add_component(
            moving,
            ComponentDesc::Body2d(Body2d {
                gravity_scale: 0.0,
                ..Default::default()
            }),
        )
        .unwrap();
    world
        .add_component(moving, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    let wall = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(2.0, 0.0, 0.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(wall, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    world.move_body_position(body, Vec2::new(4.0, 0.0)).unwrap();
    world.step_physics(0.02).unwrap();
    let position = world.local_transform(moving).unwrap().translation.x;
    assert!((position - 1.0).abs() < 1e-4, "position: {position}");
}

#[test]
fn zero_step_preserves_pending_target_and_retirement_invalidates_its_handle() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let body = world
        .add_component(
            object,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Kinematic,
                ..Default::default()
            }),
        )
        .unwrap();
    world.move_body_position(body, Vec2::new(1.0, 2.0)).unwrap();
    world.step_physics(0.0).unwrap();
    assert_eq!(
        world.local_transform(object).unwrap().translation,
        Vec3::ZERO
    );
    world.step_physics(0.02).unwrap();
    assert!(
        world
            .local_transform(object)
            .unwrap()
            .translation
            .abs_diff_eq(Vec3::new(1.0, 2.0, 0.0), 1e-5)
    );
    world.remove_component(body).unwrap();
    assert!(world.move_body_position(body, Vec2::ZERO).is_err());
}
