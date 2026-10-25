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
