use worldrunner_world_simulation::{
    Advance, ComponentDesc, DestroyWhen, FieldValue, ObjectDesc, ParentSpace, TimeSpan, World,
    WorldSettings,
};

fn script(
    w: &mut World,
    owner: worldrunner_world_simulation::ObjectHandle,
) -> worldrunner_world_simulation::ComponentHandle {
    w.add_component(
        owner,
        ComponentDesc::Script {
            assembly: "Example".into(),
            type_name: "Probe".into(),
        },
    )
    .unwrap()
}

#[test]
fn instantiate_under_pending_parent_never_activates_or_survives() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let template = w.create_object(ObjectDesc::default()).unwrap();
    let original = script(&mut w, template);
    let child = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(child, Some(template), ParentSpace::KeepLocal)
        .unwrap();
    let parent = w.create_object(ObjectDesc::default()).unwrap();
    w.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
    let clone = w.instantiate(template, Some(parent)).unwrap();
    assert!(w.take_lifecycle_work(clone.mutation).unwrap().is_empty());
    assert!(!w.script_ready(clone.components[&original]).unwrap());
    let mut advance = w.advance(TimeSpan::ZERO).unwrap();
    while let Advance::Callback(callback) = advance {
        assert!(
            !clone
                .components
                .values()
                .any(|script| *script == callback.script)
        );
        advance = w
            .resume(
                callback.ticket,
                worldrunner_world_simulation::CallbackOutcome::Returned,
            )
            .unwrap();
    }
    for object in clone.objects.values() {
        assert!(w.object(*object).is_err());
    }
    for component in clone.components.values() {
        assert!(w.component(*component).is_err());
    }
    assert!(w.object(template).is_ok());
}

#[test]
fn pending_parent_unpublished_rollback_releases_only_clone() {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let template = w.create_object(ObjectDesc::default()).unwrap();
    script(&mut w, template);
    let parent = w.create_object(ObjectDesc::default()).unwrap();
    w.destroy(parent, DestroyWhen::EndOfFrame).unwrap();
    let before = w.storage_counts();
    let clone = w.instantiate_unpublished(template, Some(parent)).unwrap();
    assert_eq!(
        w.lifecycle_work(clone.mutation),
        Err(worldrunner_world_simulation::WorldError::WrongMutation)
    );
    w.discard_instantiation(&clone).unwrap();
    assert_eq!(w.storage_counts(), before);
    w.finish_phase();
    assert!(w.object(template).is_ok());
    assert!(w.object(parent).is_err());
}

#[test]
fn pending_sources_copy_all_components_and_remap_without_inheriting_destroy() {
    for destroy_root in [false, true] {
        let mut w = World::new(WorldSettings::default()).unwrap();
        let root = w.create_object(ObjectDesc::default()).unwrap();
        let child = w.create_object(ObjectDesc::default()).unwrap();
        w.set_parent(child, Some(root), ParentSpace::KeepLocal)
            .unwrap();
        let a = script(&mut w, root);
        let b = script(&mut w, child);
        w.set_script_fields(a, vec![("child".into(), FieldValue::Component(b))])
            .unwrap();
        w.destroy(
            if destroy_root { root } else { child },
            DestroyWhen::EndOfFrame,
        )
        .unwrap();
        let clone = w.instantiate(root, None).unwrap();
        assert_eq!(clone.components.len(), 4);
        let mapped = clone.components[&a];
        assert_eq!(
            w.script_fields(mapped).unwrap()[0].1,
            FieldValue::Component(clone.components[&b])
        );
        assert!(w.script_ready(mapped).unwrap());
        w.finish_phase();
        for object in clone.objects.values() {
            assert!(w.object(*object).is_ok());
        }
        for component in clone.components.values() {
            assert!(w.component(*component).is_ok());
        }
    }
}

#[test]
fn pending_source_capacity_preflight_counts_required_transforms() {
    let mut w = World::new(WorldSettings {
        max_objects: 4,
        max_components: 3,
    })
    .unwrap();
    let root = w.create_object(ObjectDesc::default()).unwrap();
    let child = w.create_object(ObjectDesc::default()).unwrap();
    w.set_parent(child, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    w.destroy(root, DestroyWhen::EndOfFrame).unwrap();
    let before = w.snapshot();
    assert!(w.instantiate(root, None).is_err());
    assert_eq!(w.snapshot(), before);
}
