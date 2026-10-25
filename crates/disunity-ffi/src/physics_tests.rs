use crate::{abi::*, lock_store, result_code, with_native_world};
use glam::Vec2;
use worldrunner_world_simulation::{
    ComponentDesc, WorldError,
    physics2d::{Body2d, BodyType, Collider2d},
};

#[test]
fn abi_step_advances_the_same_native_physics_and_destroys_body_identity() {
    let mut world = 0;
    let mut object = 0;
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
    }
    let (native_object, body, collider) = attach_physics(world, object);
    unsafe {
        assert_eq!(disunity_world_step(world, 0.25, 1).code, result_code::OK);
    }
    with_native_world(world, |native| {
        assert!((native.local_transform(native_object).unwrap().translation.x - 0.25).abs() < 1e-6);
    })
    .unwrap();
    unsafe {
        assert_eq!(
            disunity_world_step(world, f32::NAN, 2).code,
            result_code::ERROR
        );
        assert_eq!(
            disunity_gameobject_destroy(world, object).code,
            result_code::OK
        );
    }
    with_native_world(world, |native| {
        assert_eq!(
            native.component(body),
            Err(WorldError::StaleComponent(body))
        );
        assert_eq!(
            native.component(collider),
            Err(WorldError::StaleComponent(collider))
        );
    })
    .unwrap();
    unsafe {
        assert_eq!(disunity_world_step(world, 0.0, 3).code, result_code::OK);
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

fn attach_physics(
    world: u64,
    object: u64,
) -> (
    worldrunner_world_simulation::ObjectHandle,
    worldrunner_world_simulation::ComponentHandle,
    worldrunner_world_simulation::ComponentHandle,
) {
    {
        let mut store = lock_store();
        let session = store.session_mut(world).unwrap();
        let native_object = session.object(object).unwrap();
        let body = session
            .native
            .add_component(
                native_object,
                ComponentDesc::Body2d(Body2d {
                    body_type: BodyType::Kinematic,
                    velocity: Vec2::X,
                    ..Default::default()
                }),
            )
            .unwrap();
        let collider = session
            .native
            .add_component(
                native_object,
                ComponentDesc::Collider2d(Collider2d::default()),
            )
            .unwrap();
        (native_object, body, collider)
    }
}

#[test]
fn iteration_failure_preserves_abi_edits_and_history() {
    assert_failed_step_atomic(false);
}

#[test]
fn geometry_failure_preserves_abi_edits_and_history() {
    assert_failed_step_atomic(true);
}

fn prepare_failed_step(
    world: u64,
    object: u64,
    invalid_geometry: bool,
) -> worldrunner_world_simulation::WorldSnapshot {
    let (_, body, _) = attach_physics(world, object);
    let mut store = lock_store();
    let session = store.session_mut(world).unwrap();
    if !invalid_geometry {
        let obstacle = session
            .native
            .create_object(worldrunner_world_simulation::ObjectDesc::default())
            .unwrap();
        session
            .native
            .add_component(
                obstacle,
                ComponentDesc::Collider2d(Collider2d {
                    trigger: true,
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    session
        .native
        .set_component(
            body,
            ComponentDesc::Body2d(Body2d {
                body_type: BodyType::Kinematic,
                angular_velocity: if invalid_geometry { 0.0 } else { 1.0 },
                ..Default::default()
            }),
        )
        .unwrap();
    session
        .native
        .set_physics_settings(worldrunner_world_simulation::physics2d::PhysicsSettings {
            max_iterations: 1,
            ..Default::default()
        })
        .unwrap();
    for (kind, x) in [
        (crate::command_kind::SET_MATERIAL_SLOT, 7.0),
        (crate::command_kind::SET_POSITION, 10.0),
    ] {
        session.pending_commands.push(crate::DisunityCommand {
            object,
            kind,
            x,
            y: 0.0,
            z: 0.0,
        });
    }
    if invalid_geometry {
        session.pending_commands.push(crate::DisunityCommand {
            object,
            kind: crate::command_kind::SET_ROTATION_Y,
            x: 0.5,
            y: 0.0,
            z: 0.0,
        });
    }
    session.native.snapshot()
}

fn assert_failed_step_atomic(invalid_geometry: bool) {
    let mut world = 0;
    let mut object = 0;
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
    }
    let before = prepare_failed_step(world, object, invalid_geometry);
    let pending = lock_store()
        .session(world)
        .unwrap()
        .pending_commands
        .clone();
    unsafe {
        assert_eq!(disunity_world_step(world, 0.1, 1).code, result_code::ERROR);
    }
    let (after, queued, applied, contacts) = {
        let store = lock_store();
        let session = store.session(world).unwrap();
        (
            session.native.snapshot(),
            session.pending_commands.clone(),
            session.applied_commands.clone(),
            session.native.contacts().to_vec(),
        )
    };
    assert_eq!(after, before);
    assert_eq!(queued, pending);
    assert!(applied.is_empty());
    assert!(contacts.is_empty());
    complete_retained_step(world, invalid_geometry);
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

fn complete_retained_step(world: u64, invalid_geometry: bool) {
    {
        let mut store = lock_store();
        let session = store.session_mut(world).unwrap();
        if invalid_geometry {
            session.pending_commands.pop();
        }
        session
            .native
            .set_physics_settings(
                worldrunner_world_simulation::physics2d::PhysicsSettings::default(),
            )
            .unwrap();
    }
    unsafe {
        assert_eq!(disunity_world_step(world, 0.1, 2).code, result_code::OK);
    }
    let store = lock_store();
    let session = store.session(world).unwrap();
    assert!(session.pending_commands.is_empty());
    assert_eq!(session.applied_commands.len(), 2);
    assert!(
        session
            .native
            .snapshot()
            .components
            .iter()
            .any(|c| c.desc == ComponentDesc::Renderer { material_slot: 7 })
    );
}
