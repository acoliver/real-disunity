use glam::{Vec2, Vec3};
use worldrunner_world_simulation::physics2d::{Body2d, Collider2d};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

#[test]
fn sleeping_skips_gravity_and_wakes_on_explicit_request_and_contact() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let body = world
        .add_component(object, ComponentDesc::Body2d(Body2d::default()))
        .unwrap();
    world
        .add_component(object, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    world.set_body_sleeping(body, true).unwrap();
    world.step_physics(0.1).unwrap();
    assert!(world.body_sleeping(body).unwrap());
    assert_eq!(
        world.local_transform(object).unwrap().translation,
        Vec3::ZERO
    );
    world.set_body_sleeping(body, false).unwrap();
    world.step_physics(0.1).unwrap();
    assert!(world.local_transform(object).unwrap().translation.y < 0.0);
    world
        .set_local_transform(object, LocalTransform::default())
        .unwrap();
    world.set_body_sleeping(body, true).unwrap();
    let other = world.create_object(ObjectDesc::default()).unwrap();
    let mut pose = world.local_transform(other).unwrap();
    pose.translation.x = -2.0;
    world.set_local_transform(other, pose).unwrap();
    world
        .add_component(
            other,
            ComponentDesc::Body2d(Body2d {
                velocity: Vec2::new(20.0, 0.0),
                gravity_scale: 0.0,
                ..Default::default()
            }),
        )
        .unwrap();
    world
        .add_component(other, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    world.step_physics(0.1).unwrap();
    assert!(!world.body_sleeping(body).unwrap());
    assert!(world.local_transform(object).unwrap().translation.x > 0.0);
}
