use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneGeometry, SceneParts, SceneResources, SceneRotation,
};

#[test]
fn handle_slot_reuse_increments_generation_and_keeps_scene_identity() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&scene);
    instances.mesh_count += 1;
    instances.pipelines.push(0);
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let first = instances.add(object, true).unwrap();
    let survivor = instances.add(object, true).unwrap();
    instances.remove(first).unwrap();
    let replacement = instances.add(object, true).unwrap();
    assert_eq!(replacement.slot, first.slot);
    assert_eq!(replacement.scene, first.scene);
    assert_eq!(replacement.generation, first.generation + 1);
    assert_eq!(instances.get(survivor).unwrap(), (object, true));
    assert!(matches!(
        instances.get(first),
        Err(InstanceError::StaleHandle)
    ));
}

#[test]
fn runtime_instance_edits_cannot_silently_ignore_tint() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&scene);
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_tint(worldrunner_render_plan::SceneTint::new([0; 3]).unwrap());
    instances.mesh_count = 1;
    instances.pipelines.push(0);
    let handle = instances.add(object, true).unwrap();
    assert_eq!(instances.get(handle).unwrap().0.tint(), object.tint());
    let mut batch = batch::InstanceBatch::default();
    let slot = batch.add(object, true);
    batch.set_transform(
        slot,
        SceneTransform {
            position_millimeters: [1, 2, 3],
            ..*object.transform()
        },
    );
    assert_eq!(batch.get(slot).0.tint(), object.tint());
}
