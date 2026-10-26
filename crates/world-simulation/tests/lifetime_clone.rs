#[test]
fn parent_destruction_keeps_descendants_visible_until_retirement() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    let leaf = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(leaf, Some(child), ParentSpace::KeepLocal)
        .unwrap();
    let component = script(&mut world, leaf);
    world.destroy(root, DestroyWhen::EndOfFrame).unwrap();
    world.destroy(leaf, DestroyWhen::EndOfFrame).unwrap();
    assert!(
        [root, child, leaf]
            .iter()
            .all(|&object| world.object(object).is_ok())
    );
    assert!(world.component(component).is_ok());
    assert_eq!(world.storage_counts(), (3, 4));
    assert_eq!(world.snapshot().objects.len(), 3);
    world.finish_phase();
    assert_eq!(world.storage_counts(), (0, 0));
    assert_eq!(
        world.destroy(root, DestroyWhen::EndOfFrame),
        Err(WorldError::StaleObject(root))
    );
}

#[test]
fn clone_lifecycle_uses_hierarchy_preorder_before_enable_lane() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    let sibling = world.create_object(ObjectDesc::default()).unwrap();
    let grandchild = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(sibling, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(grandchild, Some(child), ParentSpace::KeepLocal)
        .unwrap();
    let scripts = [root, child, grandchild, sibling].map(|object| script(&mut world, object));
    let clone = world.instantiate(root, None).unwrap();
    let work = world.lifecycle_work(clone.mutation).unwrap();
    let expected: Vec<_> = scripts.iter().map(|old| clone.components[old]).collect();
    assert_eq!(
        work[..4]
            .iter()
            .map(|value| value.script)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        work[..4]
            .iter()
            .all(|value| value.message == ScriptMessage::Awake)
    );
    assert!(
        work[4..]
            .iter()
            .all(|value| value.message == ScriptMessage::OnEnable)
    );
}

use worldrunner_world_simulation::{
    ComponentDesc, DestroyWhen, FieldValue, ObjectDesc, ParentSpace, ScriptMessage, World,
    WorldError, WorldSettings,
};

fn script(
    w: &mut World,
    object: worldrunner_world_simulation::ObjectHandle,
) -> worldrunner_world_simulation::ComponentHandle {
    w.add_component(
        object,
        ComponentDesc::Script {
            assembly: "Example".into(),
            type_name: "Actor".into(),
        },
    )
    .unwrap()
}

#[test]
fn clone_remaps_nested_internal_references_and_preserves_external_identity() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = w.create_object(ObjectDesc::default()).unwrap();
    let child = w.create_object(ObjectDesc::default()).unwrap();
    let external = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    let a = script(&mut w, root);
    let b = script(&mut w, child);
    let fields = vec![(
        "links".into(),
        FieldValue::Array(vec![
            FieldValue::Object(child),
            FieldValue::Component(b),
            FieldValue::Object(external),
        ]),
    )];
    w.set_script_fields(a, fields).unwrap();
    let clone = w.instantiate(root, None).unwrap();
    let mapped = clone.components[&a];
    assert_eq!(
        w.script_fields(mapped).unwrap()[0].1,
        FieldValue::Array(vec![
            FieldValue::Object(clone.objects[&child]),
            FieldValue::Component(clone.components[&b]),
            FieldValue::Object(external)
        ])
    );
    assert_eq!(w.parent(clone.objects[&child]).unwrap(), Some(clone.root));
    let work = w.lifecycle_work(clone.mutation).unwrap();
    assert_eq!(
        work.iter().map(|v| v.message).collect::<Vec<_>>(),
        vec![
            ScriptMessage::Awake,
            ScriptMessage::Awake,
            ScriptMessage::OnEnable,
            ScriptMessage::OnEnable
        ]
    );
    assert_eq!(work[0].script, mapped);
    assert!(w.script_ready(mapped).unwrap());
}

#[test]
fn inactive_clones_wait_for_activation_and_cleanup_is_exactly_once() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = w
        .create_object(ObjectDesc {
            active: false,
            ..Default::default()
        })
        .unwrap();
    let original = script(&mut w, root);
    let clone = w.instantiate(root, None).unwrap();
    let component = clone.components[&original];
    assert!(w.lifecycle_work(clone.mutation).unwrap().is_empty());
    assert!(!w.script_ready(component).unwrap());
    let mutation = w.set_active(clone.root, true).unwrap();
    assert_eq!(
        w.lifecycle_work(mutation)
            .unwrap()
            .iter()
            .map(|v| v.message)
            .collect::<Vec<_>>(),
        vec![ScriptMessage::Awake, ScriptMessage::OnEnable]
    );
    w.destroy(clone.root, DestroyWhen::EndOfFrame).unwrap();
    assert_eq!(
        w.take_cleanup()
            .iter()
            .map(|v| v.message)
            .collect::<Vec<_>>(),
        vec![ScriptMessage::OnDisable, ScriptMessage::OnDestroy]
    );
    assert!(w.take_cleanup().is_empty());
    w.finish_phase();
    assert!(w.component(component).is_err());
}

#[test]
fn destruction_stays_visible_retires_at_boundary_and_coalesces_subtrees() {
    let mut w = World::new(WorldSettings {
        max_objects: 2,
        max_components: 2,
    })
    .unwrap();
    let root = w.create_object(ObjectDesc::default()).unwrap();
    let child = w.create_object(ObjectDesc::default()).unwrap();
    let transform = w.object(child).unwrap().transform;
    w.set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    w.destroy(child, DestroyWhen::EndOfFrame).unwrap();
    w.destroy(root, DestroyWhen::EndOfFrame).unwrap();
    assert!(w.object(child).is_ok());
    assert!(w.component(transform).is_ok());
    assert!(w.create_object(ObjectDesc::default()).is_err());
    assert_eq!(w.destroy(root, DestroyWhen::EndOfFrame), Ok(()));
    assert_eq!(w.snapshot().objects.len(), 2);
    w.finish_phase();
    let replacement = w.create_object(ObjectDesc::default()).unwrap();
    assert_ne!(replacement, root);
    assert!(w.object(root).is_err());
}

#[test]
fn detached_iteration_permits_creation_and_destruction() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    for _ in 0..4 {
        w.create_object(ObjectDesc::default()).unwrap();
    }
    let snapshot = w.snapshot();
    for object in snapshot.objects {
        w.destroy(object.handle, DestroyWhen::EndOfFrame).unwrap();
        w.create_object(ObjectDesc::default()).unwrap();
    }
    assert_eq!(w.snapshot().objects.len(), 8);
    w.finish_phase();
    assert_eq!(w.snapshot().objects.len(), 4);
}

#[test]
fn clone_capacity_failure_is_atomic() {
    let mut w = World::new(WorldSettings {
        max_objects: 2,
        max_components: 3,
    })
    .unwrap();
    let root = w.create_object(ObjectDesc::default()).unwrap();
    script(&mut w, root);
    let before = w.snapshot();
    assert!(w.instantiate(root, None).is_err());
    assert_eq!(w.snapshot(), before);
}
