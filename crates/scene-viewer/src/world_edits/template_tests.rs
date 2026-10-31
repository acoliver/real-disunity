//! Edits for resolvers that return full scene objects, several per world object.

use glam::Vec3;
use worldrunner_render_plan::simulation::mesh_instances::{
    MeshExtractOptions, MeshResolver, MeshTemplate, ResolvedMesh, extract_mesh_instances,
};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneFadeBand, SceneGeometry, SceneParts,
    SceneRotation, SceneTint, SceneTransform,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, MaterialId, MeshId, MeshInstance, ObjectDesc, ObjectHandle,
    World, WorldSettings,
};

use super::*;
use crate::{RetainedHost, RetainedUpdate};

const IDENTITY: SceneTransform = SceneTransform {
    position_millimeters: [0; 3],
    rotation: SceneRotation::IDENTITY,
    scale_millionths: 1_000_000,
};

/// Draws an instance as its primary mesh, one object per LOD, and a card, each with its own
/// policies. The placeholder transform is deliberately not the identity.
struct Layered;

fn layer(index: u32) -> SceneObject {
    let placeholder = SceneTransform {
        position_millimeters: [-9, -9, -9],
        ..IDENTITY
    };
    SceneObject::new(
        placeholder,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_lod_distance_scale_millionths(1_500_000 + index)
    .unwrap()
    .with_fade(SceneFadeBand::new(1_000 * (index + 1), 2_000 * (index + 1)).unwrap())
    .with_tint(SceneTint::new([250_000, 500_000 + index, 750_000]).unwrap())
    .with_casts_shadow(index == 0)
}

impl MeshResolver for Layered {
    fn resolve(&self, _: &MeshInstance) -> Option<ResolvedMesh> {
        None
    }

    fn resolve_template(&self, instance: &MeshInstance) -> Option<MeshTemplate> {
        let count = u32::try_from(1 + instance.lods.len()).ok()?;
        Some(MeshTemplate::Objects((0..count).map(layer).collect()))
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

fn drawn_with_lods(world: &mut World, x: f32, lods: usize) -> ObjectHandle {
    let object = world
        .create_object(ObjectDesc {
            local_transform: at(x),
            ..ObjectDesc::default()
        })
        .unwrap();
    let mut instance = MeshInstance::new(MeshId(1), MaterialId(1));
    instance.lods = (0..lods as u64).map(|n| MeshId(10 + n)).collect();
    world
        .add_component(object, ComponentDesc::MeshInstance(instance))
        .unwrap();
    object
}

fn edits(world: &mut World, ids: &mut ObjectIdMap) -> Vec<SceneObjectEdit> {
    let changes = world.drain_changes();
    edits_from_world_changes(&changes, world, &Layered, ids).unwrap()
}

fn host_for(world: &mut World) -> (RetainedHost, ObjectIdMap) {
    let extracted = extract_mesh_instances(world, &Layered).unwrap();
    let base = crate::renderer::retained_tests::source();
    let scene = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(
            base.geometry().meshes().to_vec(),
            extracted.iter().map(|e| e.scene).collect(),
        )
        .unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let ids = ObjectIdMap::from_initial(extracted.iter().map(|e| e.object));
    world.drain_changes();
    (
        RetainedHost::new(scene, crate::tests::options()).unwrap(),
        ids,
    )
}

fn apply(host: &mut RetainedHost, edits: Vec<SceneObjectEdit>) {
    host.apply(RetainedUpdate::Objects(edits)).unwrap();
}

fn assert_matches_full_extraction(host: &RetainedHost, world: &World, ids: &ObjectIdMap) {
    let full = extract_mesh_instances(world, &Layered).unwrap();
    assert_eq!(host.objects().count(), full.len());
    assert_eq!(ids.id_count(), full.len());
    let mut taken: BTreeMap<ObjectHandle, usize> = BTreeMap::new();
    for extracted in full {
        let slot = taken.entry(extracted.object).or_default();
        let id = ids.get_all(extracted.object)[*slot];
        *slot += 1;
        assert_eq!(host.object(id), Some(extracted.scene));
    }
}

fn ids_of(edits: &[SceneObjectEdit]) -> Vec<ObjectId> {
    edits
        .iter()
        .map(|edit| match edit {
            SceneObjectEdit::Add { id, .. } | SceneObjectEdit::Replace { id, .. } => *id,
            SceneObjectEdit::Remove(id) => *id,
        })
        .collect()
}

#[test]
fn initial_ids_follow_the_host_scene_order() {
    let mut world = tracked_world();
    let first = drawn_with_lods(&mut world, 1.0, 2);
    let second = drawn_with_lods(&mut world, 2.0, 1);
    let (_, ids) = host_for(&mut world);
    assert_eq!(ids.get_all(first), [ObjectId(0), ObjectId(1), ObjectId(2)]);
    assert_eq!(ids.get_all(second), [ObjectId(3), ObjectId(4)]);
    assert_eq!((ids.len(), ids.id_count()), (2, 5));
    assert_eq!(ids.get(second), Some(ObjectId(3)));
}

#[test]
fn moving_replaces_every_id_and_keeps_the_template_fields() {
    let mut world = tracked_world();
    let object = drawn_with_lods(&mut world, 1.0, 2);
    let bystander = drawn_with_lods(&mut world, 5.0, 0);
    let (mut host, mut ids) = host_for(&mut world);
    let before: Vec<_> = ids
        .get_all(object)
        .iter()
        .map(|&id| host.object(id).unwrap())
        .collect();

    world.set_local_transform(object, at(8.0)).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert_eq!(ids_of(&batch), ids.get_all(object));
    assert!(batch.iter().all(|edit| matches!(
        edit,
        SceneObjectEdit::Replace { object, .. }
            if object.transform().position_millimeters == [8000, 0, 0]
    )));
    apply(&mut host, batch);

    for (&id, old) in ids.get_all(object).iter().zip(&before) {
        let now = host.object(id).unwrap();
        assert_eq!(now.transform().position_millimeters, [8000, 0, 0]);
        assert_eq!(now.with_transform(*old.transform()), *old);
    }
    assert_eq!(
        host.object(ids.get(bystander).unwrap())
            .unwrap()
            .transform()
            .position_millimeters,
        [5000, 0, 0]
    );
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn creating_adds_every_template_under_fresh_ids() {
    let mut world = tracked_world();
    drawn_with_lods(&mut world, 1.0, 1);
    let (mut host, mut ids) = host_for(&mut world);

    let added = drawn_with_lods(&mut world, 2.0, 2);
    let batch = edits(&mut world, &mut ids);
    assert!(
        batch
            .iter()
            .all(|edit| matches!(edit, SceneObjectEdit::Add { .. }))
    );
    assert_eq!(ids_of(&batch), [ObjectId(2), ObjectId(3), ObjectId(4)]);
    assert_eq!(ids.get_all(added), ids_of(&batch));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn destroying_removes_every_id_and_never_reuses_them() {
    let mut world = tracked_world();
    let doomed = drawn_with_lods(&mut world, 1.0, 2);
    drawn_with_lods(&mut world, 2.0, 0);
    let (mut host, mut ids) = host_for(&mut world);
    let released = ids.get_all(doomed).to_vec();

    world.destroy_object(doomed).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert!(
        batch
            .iter()
            .all(|edit| matches!(edit, SceneObjectEdit::Remove(_)))
    );
    assert_eq!(ids_of(&batch), released);
    assert!(ids.get_all(doomed).is_empty());
    apply(&mut host, batch);
    assert_eq!(host.objects().count(), 1);

    drawn_with_lods(&mut world, 3.0, 0);
    let batch = edits(&mut world, &mut ids);
    assert_eq!(ids_of(&batch), [ObjectId(4)]);
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn a_changed_template_count_adds_and_removes_the_difference() {
    let mut world = tracked_world();
    let object = drawn_with_lods(&mut world, 1.0, 1);
    let (mut host, mut ids) = host_for(&mut world);
    let component = world
        .snapshot()
        .components
        .into_iter()
        .find(|c| c.object == object && matches!(c.desc, ComponentDesc::MeshInstance(_)))
        .unwrap()
        .handle;
    let with_lods = |count: u64| {
        let mut instance = MeshInstance::new(MeshId(1), MaterialId(1));
        instance.lods = (0..count).map(|n| MeshId(10 + n)).collect();
        ComponentDesc::MeshInstance(instance)
    };

    world.set_component(component, with_lods(3)).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert!(matches!(
        batch.as_slice(),
        [
            SceneObjectEdit::Replace {
                id: ObjectId(0),
                ..
            },
            SceneObjectEdit::Replace {
                id: ObjectId(1),
                ..
            },
            SceneObjectEdit::Add {
                id: ObjectId(2),
                ..
            },
            SceneObjectEdit::Add {
                id: ObjectId(3),
                ..
            },
        ]
    ));
    apply(&mut host, batch);
    assert_matches_full_extraction(&host, &world, &ids);

    world.set_component(component, with_lods(0)).unwrap();
    let batch = edits(&mut world, &mut ids);
    assert!(matches!(
        batch.as_slice(),
        [
            SceneObjectEdit::Replace {
                id: ObjectId(0),
                ..
            },
            SceneObjectEdit::Remove(ObjectId(1)),
            SceneObjectEdit::Remove(ObjectId(2)),
            SceneObjectEdit::Remove(ObjectId(3)),
        ]
    ));
    apply(&mut host, batch);
    assert_eq!(ids.get_all(object), [ObjectId(0)]);
    assert_matches_full_extraction(&host, &world, &ids);
}

#[test]
fn a_millimeter_world_produces_millimeter_positions_in_edits() {
    let mut world = tracked_world();
    let object = drawn_with_lods(&mut world, 1500.0, 1);
    let mut ids = ObjectIdMap::from_initial([object, object]);
    world.drain_changes();
    world.set_local_transform(object, at(2500.0)).unwrap();
    let changes = world.drain_changes();
    let options = MeshExtractOptions::with_millimeters_per_unit(1.0).unwrap();
    let batch =
        edits_from_world_changes_with(&changes, &world, &Layered, options, &mut ids).unwrap();
    assert_eq!(batch.len(), 2);
    assert!(batch.iter().all(|edit| matches!(
        edit,
        SceneObjectEdit::Replace { object, .. }
            if object.transform().position_millimeters == [2500, 0, 0]
    )));
}
