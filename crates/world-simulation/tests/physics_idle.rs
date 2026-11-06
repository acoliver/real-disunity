//! A step in a world without 2D physics components does what the full step does with nothing to
//! simulate.

use glam::Vec2;
use worldrunner_world_simulation::physics2d::{
    Body2d, BodyType, Collider2d, ContactPhase, Interpolation, PhysicsError,
};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, DestroyWhen, ObjectDesc, ObjectHandle, World, WorldError,
    WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn plain_object(world: &mut World) -> ObjectHandle {
    world.create_object(ObjectDesc::default()).unwrap()
}

fn overlapping_pair(world: &mut World) -> [ComponentHandle; 3] {
    let a = plain_object(world);
    let b = plain_object(world);
    let body = world
        .add_component(
            a,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Dynamic,
                gravity_scale: 0.0,
                ..Default::default()
            }),
        )
        .unwrap();
    let first = world
        .add_component(a, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    let second = world
        .add_component(b, ComponentDesc::Collider2d(Collider2d::default()))
        .unwrap();
    [body, first, second]
}

#[test]
fn a_world_without_physics_steps_with_no_contacts() {
    let mut world = world();
    plain_object(&mut world);
    for _ in 0..3 {
        world.step_physics(0.02).unwrap();
        assert!(world.contacts().is_empty());
        assert!(world.next_contact().is_none());
    }
    world.step_physics(0.0).unwrap();
}

#[test]
fn an_invalid_delta_still_errors_without_physics() {
    let mut world = world();
    for dt in [f32::NAN, -0.01, f32::INFINITY] {
        assert_eq!(
            world.step_physics(dt),
            Err(WorldError::Physics(PhysicsError::InvalidDelta))
        );
    }
}

#[test]
fn colliders_added_after_idle_steps_make_contacts() {
    let mut world = world();
    world.step_physics(0.02).unwrap();
    world.step_physics(0.02).unwrap();
    overlapping_pair(&mut world);
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts().len(), 1);
    assert_eq!(world.contacts()[0].phase, ContactPhase::Enter);
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts()[0].phase, ContactPhase::Stay);
}

#[test]
fn removing_the_last_physics_components_ends_contacts_in_the_next_step() {
    let mut world = world();
    let parts = overlapping_pair(&mut world);
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts()[0].phase, ContactPhase::Enter);
    for part in parts {
        world.remove_component(part).unwrap();
    }
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts().len(), 1);
    assert_eq!(world.contacts()[0].phase, ContactPhase::Exit);
    assert!(world.next_contact().is_none());
    world.step_physics(0.02).unwrap();
    assert!(world.contacts().is_empty());
}

#[test]
fn destroying_the_owner_of_the_last_physics_components_ends_contacts() {
    let mut world = world();
    let a = plain_object(&mut world);
    let b = plain_object(&mut world);
    for object in [a, b] {
        world
            .add_component(object, ComponentDesc::Collider2d(Collider2d::default()))
            .unwrap();
    }
    world
        .add_component(
            a,
            ComponentDesc::Body2d(Body2d {
                gravity_scale: 0.0,
                ..Default::default()
            }),
        )
        .unwrap();
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts().len(), 1);
    world.destroy_object(a).unwrap();
    world.destroy(b, DestroyWhen::EndOfFrame).unwrap();
    world.step_physics(0.02).unwrap();
    assert_eq!(world.contacts()[0].phase, ContactPhase::Exit);
    world.finish_phase();
    world.step_physics(0.02).unwrap();
    assert!(world.contacts().is_empty());
}

#[test]
fn removing_the_last_interpolated_body_drops_its_step_poses() {
    let mut world = world();
    let object = plain_object(&mut world);
    let body = world
        .add_component(
            object,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Dynamic,
                interpolation: Interpolation::Interpolate,
                gravity_scale: 0.0,
                velocity: Vec2::X,
                ..Default::default()
            }),
        )
        .unwrap();
    world.step_physics(0.02).unwrap();
    world.presented_matrix(object).unwrap();
    world.remove_component(body).unwrap();
    world.step_physics(0.02).unwrap();
    assert_eq!(
        world.presented_matrix(object).unwrap(),
        world.world_matrix(object).unwrap()
    );
}
