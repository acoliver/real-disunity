use glam::Vec3;
use worldrunner_render_plan::simulation::mesh_instances::{ResolvedMesh, extract_mesh_instances};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneGeometry, SceneObject, SceneParts,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldSettings,
};

use super::*;
use crate::{RetainedHost, RetainedUpdate};

/// The synthetic scene has one mesh and one material, so every instance resolves to them and
/// objects differ only by transform.
fn resolver(instance: &MeshInstance) -> Option<ResolvedMesh> {
    (instance.mesh == MeshId(1)).then_some(ResolvedMesh {
        mesh: MeshHandle::from_raw(0),
        material: MaterialHandle::from_raw(0),
    })
}

fn tracked_world() -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.track_changes();
    world
}

fn at(x: f32) -> ObjectDesc {
    ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(x, 0.0, 0.0),
            ..LocalTransform::default()
        },
        ..ObjectDesc::default()
    }
}

fn drawn(world: &mut World, x: f32) -> ObjectHandle {
    let object = world.create_object(at(x)).unwrap();
    show(world, object);
    object
}

fn show(world: &mut World, object: ObjectHandle) {
    let instance = MeshInstance::new(MeshId(1), MaterialId(1));
    world
        .add_component(object, ComponentDesc::MeshInstance(instance))
        .unwrap();
}

fn edits(world: &mut World, ids: &mut ObjectIdMap) -> Vec<SceneObjectEdit> {
    let changes = world.drain_changes();
    edits_from_world_changes(&changes, world, &resolver, ids).unwrap()
}

/// A host whose initial scene is the full extraction of `world`, plus its id map.
fn host_for(world: &mut World) -> (RetainedHost, ObjectIdMap) {
    let objects: Vec<_> = extract_mesh_instances(world, &resolver)
        .unwrap()
        .into_iter()
        .collect();
    let base = crate::renderer::retained_tests::source();
    let scene = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(
            base.geometry().meshes().to_vec(),
            objects.iter().map(|extracted| extracted.scene).collect(),
        )
        .unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let ids = ObjectIdMap::from_initial(objects.iter().map(|extracted| extracted.object));
    world.drain_changes();
    (
        RetainedHost::new(scene, crate::tests::options()).unwrap(),
        ids,
    )
}

/// The host shows exactly what a fresh full extraction of the world would.
fn assert_matches_full_extraction(host: &RetainedHost, world: &World, ids: &ObjectIdMap) {
    let full = extract_mesh_instances(world, &resolver).unwrap();
    assert_eq!(host.objects().count(), full.len());
    assert_eq!(ids.len(), full.len());
    for extracted in full {
        let id = ids.get(extracted.object).expect("drawn object has an id");
        assert_eq!(host.object(id), Some(extracted.scene));
    }
}

fn apply(host: &mut RetainedHost, edits: Vec<SceneObjectEdit>) {
    host.apply(RetainedUpdate::Objects(edits)).unwrap();
}

fn x_of(object: &SceneObject) -> i64 {
    object.transform().position_millimeters[0]
}

#[test]
fn an_empty_journal_gives_no_edits() {
    let mut world = tracked_world();
    drawn(&mut world, 1.0);
    let (_, mut ids) = host_for(&mut world);
    assert!(edits(&mut world, &mut ids).is_empty());
}

#[test]
fn creating_a_drawn_object_adds_it_under_a_fresh_id() {
    let mut world = tracked_world();
    drawn(&mut world, 1.0);
    let (mut host, mut ids) = host_for(&mut world);

    world.create_object(at(9.0)).unwrap();
    assert!(
        edits(&mut world, &mut ids).is_empty(),
        "no mesh instance, nothing to draw"
    );

    let added = drawn(&mut world, 2.0);
    let batch = edits(&mut world, &mut ids);
    assert!(matches!(
        batch.as_slice(),
        [SceneObjectEdit::Add { id: ObjectId(1), object }] if x_of(object) == 2000
    ));
    assert_eq!(ids.get(added), Some(ObjectId(1)));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn moving_a_parent_replaces_it_and_its_children() {
    let mut world = tracked_world();
    let parent = drawn(&mut world, 1.0);
    let child = drawn(&mut world, 0.5);
    let bystander = drawn(&mut world, 7.0);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    let (mut host, mut ids) = host_for(&mut world);

    world
        .set_local_transform(parent, at(10.0).local_transform)
        .unwrap();
    let batch = edits(&mut world, &mut ids);
    let replaced: Vec<_> = batch
        .iter()
        .map(|edit| match edit {
            SceneObjectEdit::Replace { id, object } => (*id, x_of(object)),
            other => panic!("expected only replaces, got {other:?}"),
        })
        .collect();
    assert_eq!(
        replaced,
        [
            (ids.get(parent).unwrap(), 10_000),
            (ids.get(child).unwrap(), 10_500)
        ]
    );
    assert!(batch.iter().all(|edit| !matches!(edit, SceneObjectEdit::Replace { id, .. } if *id == ids.get(bystander).unwrap())));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn reparenting_replaces_the_object_at_its_new_world_position() {
    let mut world = tracked_world();
    let anchor = world.create_object(at(100.0)).unwrap();
    let object = drawn(&mut world, 1.0);
    let (mut host, mut ids) = host_for(&mut world);

    world
        .set_parent(object, Some(anchor), ParentSpace::KeepLocal)
        .unwrap();
    let batch = edits(&mut world, &mut ids);
    assert!(matches!(
        batch.as_slice(),
        [SceneObjectEdit::Replace { object, .. }] if x_of(object) == 101_000
    ));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn destroying_and_hiding_objects_remove_them() {
    let mut world = tracked_world();
    let destroyed = drawn(&mut world, 1.0);
    let hidden = drawn(&mut world, 2.0);
    let stripped = drawn(&mut world, 3.0);
    let kept = drawn(&mut world, 4.0);
    let (mut host, mut ids) = host_for(&mut world);
    let (destroyed_id, hidden_id, stripped_id) = (
        ids.get(destroyed).unwrap(),
        ids.get(hidden).unwrap(),
        ids.get(stripped).unwrap(),
    );

    world.destroy_object(destroyed).unwrap();
    world.set_active(hidden, false).unwrap();
    let component = world
        .snapshot()
        .components
        .into_iter()
        .find(|c| c.object == stripped && matches!(c.desc, ComponentDesc::MeshInstance(_)))
        .unwrap()
        .handle;
    world.remove_component(component).unwrap();

    let batch = edits(&mut world, &mut ids);
    let removed: BTreeSet<_> = batch
        .iter()
        .map(|edit| match edit {
            SceneObjectEdit::Remove(id) => *id,
            other => panic!("expected only removes, got {other:?}"),
        })
        .collect();
    assert_eq!(
        removed,
        BTreeSet::from([destroyed_id, hidden_id, stripped_id])
    );
    apply(&mut host, batch);
    assert_eq!(host.objects().count(), 1);
    assert_matches_full_extraction(&host, &world, &ids);
    assert!(ids.get(kept).is_some() && ids.get(hidden).is_none());

    world.set_active(hidden, true).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert!(matches!(batch.as_slice(), [SceneObjectEdit::Add { .. }]));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn a_destroyed_subtree_is_removed_once() {
    let mut world = tracked_world();
    let parent = drawn(&mut world, 1.0);
    let child = drawn(&mut world, 2.0);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    let (mut host, mut ids) = host_for(&mut world);

    world.destroy_object(parent).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert_eq!(batch.len(), 2);
    apply(&mut host, batch);
    assert_eq!(host.objects().count(), 0);
    assert!(ids.is_empty());
}

#[test]
fn a_mixed_frame_stays_equal_to_full_extraction() {
    let mut world = tracked_world();
    let a = drawn(&mut world, 1.0);
    let b = drawn(&mut world, 2.0);
    let c = drawn(&mut world, 3.0);
    let (mut host, mut ids) = host_for(&mut world);

    for frame in 0..3_u8 {
        let x = f32::from(frame) * 5.0;
        world.set_local_transform(a, at(x).local_transform).unwrap();
        world
            .set_parent(b, Some(a), ParentSpace::KeepLocal)
            .unwrap();
        if frame == 1 {
            world.destroy_object(c).unwrap();
        }
        drawn(&mut world, 20.0 + x);
        let batch = edits(&mut world, &mut ids);
        apply(&mut host, batch);
        assert_matches_full_extraction(&host, &world, &ids);
    }
}

#[test]
fn a_resolution_failure_leaves_the_id_map_unchanged() {
    let mut world = tracked_world();
    drawn(&mut world, 1.0);
    let (_, mut ids) = host_for(&mut world);
    let before = ids.len();

    let object = world.create_object(at(2.0)).unwrap();
    let unknown = MeshInstance::new(MeshId(99), MaterialId(1));
    world
        .add_component(object, ComponentDesc::MeshInstance(unknown))
        .unwrap();
    let changes = world.drain_changes();
    assert!(edits_from_world_changes(&changes, &world, &resolver, &mut ids).is_err());
    assert_eq!(ids.len(), before);
    assert_eq!(ids.get(object), None);
}
