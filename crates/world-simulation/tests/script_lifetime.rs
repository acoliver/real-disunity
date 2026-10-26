#[test]
fn cloning_copies_pending_components_without_inheriting_their_retirement() {
    let (mut world, object, script) = setup();
    world
        .set_script_fields(script, vec![("self".into(), FieldValue::Component(script))])
        .unwrap();
    world.destroy_component(script).unwrap();
    let clone = world.instantiate(object, None).unwrap();
    let copied = clone.components[&script];
    assert_eq!(
        world.script_fields(copied).unwrap()[0].1,
        FieldValue::Component(copied)
    );
    assert_eq!(
        world
            .lifecycle_work(clone.mutation)
            .unwrap()
            .iter()
            .map(|work| work.message)
            .collect::<Vec<_>>(),
        [ScriptMessage::Awake, ScriptMessage::OnEnable]
    );
    assert!(world.component(script).is_ok());
    assert!(!world.script_ready(script).unwrap());
    assert!(world.script_ready(copied).unwrap());
    world.finish_phase();
    assert!(world.component(script).is_err());
    assert!(world.component(copied).is_ok());
    assert!(world.object(object).is_ok());
    assert!(world.object(clone.root).is_ok());
}

use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, DestroyWhen, FieldValue, ObjectDesc, ScriptMessage, World,
    WorldError, WorldSettings,
};

fn setup() -> (
    World,
    worldrunner_world_simulation::ObjectHandle,
    worldrunner_world_simulation::ComponentHandle,
) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let script = world
        .add_component(
            object,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap();
    (world, object, script)
}

#[test]
fn deferred_component_removal_suppresses_readiness_and_emits_cleanup_once() {
    let (mut world, object, script) = setup();
    world.set_active(object, true).unwrap();
    world.destroy_component(script).unwrap();
    assert!(world.component(script).is_ok());
    assert!(!world.script_ready(script).unwrap());
    assert_eq!(world.storage_counts(), (1, 2));
    assert_eq!(
        world
            .take_cleanup()
            .iter()
            .map(|v| v.message)
            .collect::<Vec<_>>(),
        vec![ScriptMessage::OnDisable, ScriptMessage::OnDestroy]
    );
    world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
    assert!(world.take_cleanup().is_empty());
    world.finish_phase();
    assert_eq!(world.storage_counts(), (0, 0));
}

#[test]
fn disabled_clone_awakes_without_enable_and_keeps_external_component_reference() {
    let (mut world, object, script) = setup();
    let external = world.create_object(ObjectDesc::default()).unwrap();
    let transform = world.object(external).unwrap().transform;
    world
        .set_script_fields(
            script,
            vec![(
                "record".into(),
                FieldValue::Record(vec![("external".into(), FieldValue::Component(transform))]),
            )],
        )
        .unwrap();
    world.set_script_enabled(script, false).unwrap();
    let clone = world.instantiate(object, None).unwrap();
    assert_eq!(
        world
            .lifecycle_work(clone.mutation)
            .unwrap()
            .iter()
            .map(|v| v.message)
            .collect::<Vec<_>>(),
        vec![ScriptMessage::Awake]
    );
    assert!(!world.script_ready(clone.components[&script]).unwrap());
    assert_eq!(
        world.script_fields(script).unwrap(),
        world.script_fields(clone.components[&script]).unwrap()
    );
}

#[test]
fn invalid_fields_and_required_transform_removal_are_atomic() {
    let (mut world, object, script) = setup();
    let foreign = World::new(WorldSettings::default()).unwrap();
    assert!(
        foreign
            .lifecycle_work(world.set_active(object, true).unwrap())
            .is_err()
    );
    let before = world.snapshot();
    let transform = world.object(object).unwrap().transform;
    assert_eq!(
        world.destroy_component(transform),
        Err(WorldError::InvalidComponent(
            ComponentError::RequiredTransform
        ))
    );
    assert_eq!(
        world.set_script_fields(script, vec![("x".into(), FieldValue::Number(f64::NAN))]),
        Err(WorldError::InvalidFields)
    );
    let mut value = FieldValue::Null;
    for _ in 0..34 {
        value = FieldValue::Array(vec![value]);
    }
    assert_eq!(
        world.set_script_fields(script, vec![("deep".into(), value)]),
        Err(WorldError::InvalidFields)
    );
    assert_eq!(world.snapshot(), before);
    assert!(world.script_fields(script).unwrap().is_empty());
}
