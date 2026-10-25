use worldrunner_world_simulation::physics2d::{Body2d, BodyType, Collider2d};
use worldrunner_world_simulation::*;
#[test]
fn native_contact_callbacks_follow_physics_and_recheck_mutation() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let (scripts, colliders) = attach_pair(&mut world);
    let mut work = world.advance(TimeSpan::FIXED).unwrap();
    let mut contacts = 0;
    let mut update = false;
    loop {
        match work {
            Advance::Callback(c) => {
                if let ScriptMessage::Contact2d(id) = c.message {
                    assert!(!update);
                    assert_eq!(c.script, scripts[0]);
                    assert!(world.contact_event(id).unwrap().trigger);
                    contacts += 1;
                    world
                        .set_component(
                            colliders[1],
                            ComponentDesc::Collider2d(Collider2d {
                                enabled: false,
                                ..Default::default()
                            }),
                        )
                        .unwrap();
                }
                if c.message == ScriptMessage::Update {
                    update = true;
                }
                work = world.resume(c.ticket, CallbackOutcome::Returned).unwrap();
            }
            Advance::Complete(_) => break,
            Advance::LifecycleComplete => panic!(),
        }
    }
    assert_eq!(contacts, 1);
    assert!(update);
}

fn attach_pair(world: &mut World) -> (Vec<ComponentHandle>, Vec<ComponentHandle>) {
    let mut scripts = Vec::new();
    let mut colliders = Vec::new();
    for index in 0..2 {
        let object = world.create_object(ObjectDesc::default()).unwrap();
        scripts.push(
            world
                .add_component(
                    object,
                    ComponentDesc::Script {
                        assembly: "Example".into(),
                        type_name: "Actor".into(),
                    },
                )
                .unwrap(),
        );
        colliders.push(
            world
                .add_component(
                    object,
                    ComponentDesc::Collider2d(Collider2d {
                        trigger: true,
                        ..Default::default()
                    }),
                )
                .unwrap(),
        );
        if index == 0 {
            world
                .add_component(
                    object,
                    ComponentDesc::Body2d(Body2d {
                        body_type: BodyType::Kinematic,
                        ..Default::default()
                    }),
                )
                .unwrap();
        }
    }
    (scripts, colliders)
}
