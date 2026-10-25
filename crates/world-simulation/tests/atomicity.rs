use glam::Vec3;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, LimitKind, ObjectDesc, ObjectEdit, World, WorldError,
    WorldSettings,
};

#[test]
fn batch_rejects_stale_targets_before_any_write() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let live = world.create_object(ObjectDesc::default()).unwrap();
    let stale = world.create_object(ObjectDesc::default()).unwrap();
    world.destroy_object(stale).unwrap();
    let before = world.snapshot();
    let edits = [
        ObjectEdit::Translation(live, Vec3::ONE),
        ObjectEdit::MaterialSlot(stale, 4),
    ];
    assert_eq!(
        world.apply_edits(&edits),
        Err(WorldError::StaleObject(stale))
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn batch_capacity_failure_does_not_publish_a_partial_component() {
    let mut world = World::new(WorldSettings {
        max_objects: 2,
        max_components: 3,
    })
    .unwrap();
    let a = world.create_object(ObjectDesc::default()).unwrap();
    let b = world.create_object(ObjectDesc::default()).unwrap();
    let before = world.snapshot();
    assert_eq!(
        world.apply_edits(&[
            ObjectEdit::MaterialSlot(a, 1),
            ObjectEdit::MaterialSlot(b, 2)
        ]),
        Err(WorldError::CapacityExceeded(LimitKind::Components))
    );
    assert_eq!(world.snapshot(), before);
    world
        .apply_edits(&[
            ObjectEdit::MaterialSlot(a, 1),
            ObjectEdit::MaterialSlot(a, 2),
        ])
        .unwrap();
    assert_eq!(
        world
            .component(world.renderer(a).unwrap().unwrap())
            .unwrap()
            .desc,
        ComponentDesc::Renderer { material_slot: 2 }
    );
}

#[test]
fn component_reuse_and_invalid_replacement_preserve_identity_rules() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let old = world
        .add_component(object, ComponentDesc::Renderer { material_slot: 1 })
        .unwrap();
    world.remove_component(old).unwrap();
    let new = world
        .add_component(object, ComponentDesc::Renderer { material_slot: 2 })
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(world.component(old), Err(WorldError::StaleComponent(old)));
    let before = world.snapshot();
    let script = ComponentDesc::Script {
        assembly: "Example".into(),
        type_name: "Mover".into(),
    };
    assert_eq!(
        world.set_component(new, script.clone()),
        Err(WorldError::InvalidComponent(ComponentError::KindMismatch))
    );
    assert_eq!(world.snapshot(), before);
    let handle = world.add_component(object, script.clone()).unwrap();
    assert_eq!(world.component(handle).unwrap().desc, script);
}

#[test]
fn invalid_object_and_component_descriptions_leave_no_allocations() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let invalid = ObjectDesc {
        layer: 32,
        ..Default::default()
    };
    assert_eq!(world.create_object(invalid), Err(WorldError::InvalidObject));
    assert!(world.snapshot().objects.is_empty());
    assert!(world.snapshot().components.is_empty());
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let before = world.snapshot();
    assert_eq!(
        world.add_component(
            object,
            ComponentDesc::Script {
                assembly: String::new(),
                type_name: "Mover".into()
            }
        ),
        Err(WorldError::InvalidComponent(ComponentError::InvalidScript))
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn object_capacity_failure_and_stale_renderer_lookup_are_explicit() {
    let mut world = World::new(WorldSettings {
        max_objects: 1,
        max_components: 2,
    })
    .unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let before = world.snapshot();
    assert_eq!(
        world.create_object(ObjectDesc::default()),
        Err(WorldError::CapacityExceeded(LimitKind::Objects))
    );
    assert_eq!(world.snapshot(), before);
    world.destroy_object(object).unwrap();
    assert_eq!(world.renderer(object), Err(WorldError::StaleObject(object)));
}
