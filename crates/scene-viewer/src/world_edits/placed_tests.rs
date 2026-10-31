//! Edits for resolvers that return placed templates: the host computes the scene transform and
//! the world journal only decides which objects to extract again.

use glam::Vec3;
use worldrunner_render_plan::simulation::mesh_instances::{
    MeshResolver, MeshTemplate, ResolvedMesh,
};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneObject, SceneRotation, SceneTransform,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle,
    ParentSpace, World, WorldSettings,
};

use super::*;

/// The host's placement policy: the position comes from the instance's mesh id and the object
/// index, the rotation and scale are fixed. The world transform plays no part.
struct Placer;

fn placement(mesh: u64, index: i64) -> SceneObject {
    let transform = SceneTransform {
        position_millimeters: [i64::try_from(mesh).unwrap() * 100 + index, 7, -7],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 707_107,
            w: 707_107,
        },
        scale_millionths: 1_250_000,
    };
    SceneObject::new(
        transform,
        MeshHandle::from_raw(u32::try_from(index).unwrap()),
        MaterialHandle::from_raw(0),
    )
    .with_casts_shadow(index == 0)
}

impl MeshResolver for Placer {
    fn resolve(&self, _: &MeshInstance) -> Option<ResolvedMesh> {
        None
    }

    fn resolve_template(&self, instance: &MeshInstance) -> Option<MeshTemplate> {
        let count = i64::try_from(1 + instance.lods.len()).ok()?;
        Some(MeshTemplate::Placed(
            (0..count)
                .map(|index| placement(instance.mesh.0, index))
                .collect(),
        ))
    }
}

fn tracked_world() -> World {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.track_changes();
    world
}

fn at(x: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3::new(x, 0.0, 0.0),
        ..LocalTransform::default()
    }
}

fn placed(world: &mut World, mesh: u64, lods: u64, local: LocalTransform) -> ObjectHandle {
    let object = world
        .create_object(ObjectDesc {
            local_transform: local,
            ..ObjectDesc::default()
        })
        .unwrap();
    let mut instance = MeshInstance::new(MeshId(mesh), MaterialId(1));
    instance.lods = (0..lods).map(|n| MeshId(900 + n)).collect();
    world
        .add_component(object, ComponentDesc::MeshInstance(instance))
        .unwrap();
    object
}

/// A comparable image of [`SceneObjectEdit`], which has no equality of its own.
#[derive(Debug, PartialEq)]
enum Edit {
    Add(ObjectId, SceneObject),
    Replace(ObjectId, SceneObject),
    Remove(ObjectId),
}

fn edits(world: &mut World, ids: &mut ObjectIdMap) -> Vec<Edit> {
    let changes = world.drain_changes();
    edits_from_world_changes(&changes, world, &Placer, ids)
        .unwrap()
        .into_iter()
        .map(|edit| match edit {
            SceneObjectEdit::Add { id, object } => Edit::Add(id, object),
            SceneObjectEdit::Replace { id, object } => Edit::Replace(id, object),
            SceneObjectEdit::Remove(id) => Edit::Remove(id),
        })
        .collect()
}

fn wanted(mesh: u64, count: i64) -> Vec<SceneObject> {
    (0..count).map(|index| placement(mesh, index)).collect()
}

#[test]
fn creating_adds_the_host_objects_unchanged_under_fresh_ids() {
    let mut world = tracked_world();
    let mut ids = ObjectIdMap::default();
    let object = placed(&mut world, 3, 2, at(42.0));
    let batch = edits(&mut world, &mut ids);
    let expected: Vec<_> = wanted(3, 3)
        .into_iter()
        .zip(0..)
        .map(|(object, id)| Edit::Add(ObjectId(id), object))
        .collect();
    assert_eq!(batch, expected);
    assert_eq!(ids.get_all(object), [ObjectId(0), ObjectId(1), ObjectId(2)]);
}

#[test]
fn a_world_transform_change_replaces_every_id_with_the_unchanged_host_objects() {
    let mut world = tracked_world();
    let mut ids = ObjectIdMap::default();
    let moved = placed(&mut world, 3, 1, at(1.0));
    placed(&mut world, 4, 0, at(2.0));
    edits(&mut world, &mut ids);

    world.set_local_transform(moved, at(500.0)).unwrap();
    let batch = edits(&mut world, &mut ids);
    let expected: Vec<_> = wanted(3, 2)
        .into_iter()
        .zip(0..)
        .map(|(object, id)| Edit::Replace(ObjectId(id), object))
        .collect();
    assert_eq!(batch, expected);
}

#[test]
fn moving_a_parent_replaces_its_placed_descendants() {
    let mut world = tracked_world();
    let mut ids = ObjectIdMap::default();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            child,
            ComponentDesc::MeshInstance(MeshInstance::new(MeshId(5), MaterialId(1))),
        )
        .unwrap();
    edits(&mut world, &mut ids);

    world.set_local_transform(parent, at(9.0)).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert_eq!(
        batch,
        [Edit::Replace(ids.get(child).unwrap(), placement(5, 0))]
    );
}

#[test]
fn destroying_removes_every_id_and_never_reuses_them() {
    let mut world = tracked_world();
    let mut ids = ObjectIdMap::default();
    let doomed = placed(&mut world, 3, 2, at(1.0));
    edits(&mut world, &mut ids);

    world.destroy_object(doomed).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert_eq!(
        batch,
        [
            Edit::Remove(ObjectId(0)),
            Edit::Remove(ObjectId(1)),
            Edit::Remove(ObjectId(2)),
        ]
    );
    assert!(ids.is_empty());

    placed(&mut world, 6, 0, at(3.0));
    let batch = edits(&mut world, &mut ids);
    assert_eq!(batch, [Edit::Add(ObjectId(3), placement(6, 0))]);
}

#[test]
fn deactivating_removes_and_reactivating_adds_the_placed_objects() {
    let mut world = tracked_world();
    let mut ids = ObjectIdMap::default();
    let object = placed(&mut world, 3, 0, at(1.0));
    edits(&mut world, &mut ids);

    world.set_active(object, false).unwrap();
    assert_eq!(edits(&mut world, &mut ids), [Edit::Remove(ObjectId(0))]);
    world.set_active(object, true).unwrap();
    assert_eq!(
        edits(&mut world, &mut ids),
        [Edit::Add(ObjectId(1), placement(3, 0))]
    );
}
