use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ParentSpace, ScriptMessage, World, WorldSettings,
};

#[test]
fn b3_reparenting_returns_disable_enable_and_first_activation_work() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let inactive = world
        .create_object(ObjectDesc {
            active: false,
            ..Default::default()
        })
        .unwrap();
    let template = world.create_object(ObjectDesc::default()).unwrap();
    let script = world
        .add_component(
            template,
            ComponentDesc::Script {
                assembly: "Example".into(),
                type_name: "Actor".into(),
            },
        )
        .unwrap();
    let clone = world.instantiate(template, None).unwrap();
    let cloned_script = clone.components[&script];
    let disabled = world
        .set_parent(clone.root, Some(inactive), ParentSpace::KeepLocal)
        .unwrap();
    let work = world.lifecycle_work(disabled).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].script, cloned_script);
    assert_eq!(work[0].message, ScriptMessage::OnDisable);
    let enabled = world
        .set_parent(clone.root, None, ParentSpace::KeepLocal)
        .unwrap();
    let work = world.lifecycle_work(enabled).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].message, ScriptMessage::OnEnable);
    let waiting = world.instantiate(template, Some(inactive)).unwrap();
    assert!(world.lifecycle_work(waiting.mutation).unwrap().is_empty());
    let activated = world
        .set_parent(waiting.root, None, ParentSpace::KeepLocal)
        .unwrap();
    assert_eq!(
        world
            .lifecycle_work(activated)
            .unwrap()
            .iter()
            .map(|w| w.message)
            .collect::<Vec<_>>(),
        vec![ScriptMessage::Awake, ScriptMessage::OnEnable]
    );
}
