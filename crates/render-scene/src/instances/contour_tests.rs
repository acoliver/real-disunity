use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneContourStyle, SceneContours, SceneGeometry, SceneParts,
    SceneResources, SceneRotation,
};

fn empty() -> Instances {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&scene);
    instances.mesh_count = 1;
    instances.pipelines.push(0);
    instances
}

fn object() -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

fn styles() -> SceneContours {
    let mut styles = vec![None; 256];
    styles[255] = Some(SceneContourStyle::default());
    SceneContours::new([1920, 1080], styles.clone()).unwrap()
}

#[test]
fn contour_atomic_edits_membership_removal_and_slot_reuse() {
    let mut instances = empty();
    let selected = object().with_contour_id(255);
    assert!(instances.add(selected, true).is_err());
    assert!(instances.contour_members.is_empty());
    instances.set_contours(styles()).unwrap();
    let handle = instances.add(object(), true).unwrap();
    let original_bytes = instances.batches.values().next().unwrap().bytes.clone();
    instances.set_object(handle, selected).unwrap();
    assert_eq!(instances.contour_members.len(), 1);
    assert_eq!(
        original_bytes,
        instances.batches.values().next().unwrap().bytes
    );
    let members = instances.contour_members.clone();
    assert!(
        instances
            .set_object(handle, object().with_contour_id(1))
            .is_err()
    );
    assert!(instances.set_contours(SceneContours::default()).is_err());
    assert_eq!(instances.contours, styles());
    assert_eq!(instances.contour_members, members);
    assert_eq!(instances.get(handle).unwrap().0, selected);
    let invalid = selected.with_transform(SceneTransform {
        scale_millionths: 0,
        ..*selected.transform()
    });
    assert!(instances.set_object(handle, invalid).is_err());
    assert_eq!(instances.contour_members, members);
    instances.set_visible(handle, false).unwrap();
    assert!(
        instances.set_contours(SceneContours::default()).is_err(),
        "hidden live references remain defined"
    );
    instances.remove(handle).unwrap();
    assert!(instances.contour_members.is_empty());
    instances.set_contours(SceneContours::default()).unwrap();
    let reused = instances.add(object(), true).unwrap();
    assert_eq!(reused.slot, handle.slot);
    assert!(instances.contour_members.is_empty());
    instances.set_contours(styles()).unwrap();
    instances.set_object(reused, selected).unwrap();
    instances.set_object(reused, object()).unwrap();
    assert!(instances.contour_members.is_empty());
}
