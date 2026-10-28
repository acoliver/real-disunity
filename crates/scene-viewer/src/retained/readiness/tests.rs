use super::*;
use std::time::Duration;

fn host(limit: usize) -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::source(),
        crate::tests::options(),
        limit,
    )
    .unwrap()
}

fn camera() -> RetainedUpdate {
    let options = crate::tests::options();
    RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: options.pose,
        lens: options.camera,
    }
}

#[test]
fn pressure_is_typed_and_leaves_all_state_unchanged() {
    let mut host = host(2);
    host.apply(camera()).unwrap();
    host.apply(camera()).unwrap();
    let scene = host.scene.clone();
    for _ in 0..100 {
        assert!(matches!(
            host.apply(camera()),
            Err(ViewerError::BackPressure { limit: 2 })
        ));
    }
    assert_eq!(host.preparations_in_flight(), 2);
    assert_eq!(host.accepted_generation(), 2);
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(host.scene, scene);
    assert_eq!(host.readiness.queue.len(), 2);
}

#[test]
fn generation_order_is_acceptance_order_even_when_later_preparation_finishes_first() {
    let mut host = host(3);
    let update = RetainedUpdate::Resources {
        resources: host.scene.resources().clone(),
        meshes: host.scene.geometry().meshes().to_vec(),
    };
    host.apply(update).unwrap();
    host.apply(camera()).unwrap();
    assert_eq!(host.readiness.queue[0].generation, 1);
    assert_eq!(host.readiness.queue[1].generation, 2);
    assert!(host.readiness.queue[1].scene.is_none());
    let prepared = host.readiness.queue[0]
        .scene
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(prepared.scene(), &host.scene);
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(host.preparations_in_flight(), 2);
    host.readiness.publish_front();
    assert_eq!(host.ready_generation(), 1);
    host.readiness.publish_front();
    assert_eq!(host.ready_generation(), 2);
    assert_eq!(host.preparations_in_flight(), 0);
}

#[test]
fn replacement_tables_allow_new_meshes_materials_and_objects() {
    let mut host = host(4);
    let mut meshes = host.scene.geometry().meshes().to_vec();
    meshes.push(meshes[0].clone());
    let mut materials = host.scene.resources().materials().to_vec();
    materials.push(materials[0]);
    let resources =
        SceneResources::new(host.scene.resources().textures().to_vec(), materials).unwrap();
    host.apply(RetainedUpdate::Resources { resources, meshes })
        .unwrap();
    let original = host.object(ObjectId(0)).unwrap();
    let object = SceneObject::new(
        *original.transform(),
        worldrunner_render_plan::MeshHandle::from_raw(1),
        worldrunner_render_plan::MaterialHandle::from_raw(1),
    );
    host.apply(RetainedUpdate::Add {
        id: ObjectId(10),
        object,
    })
    .unwrap();
    host.apply(RetainedUpdate::Replace {
        id: ObjectId(0),
        object,
    })
    .unwrap();
    assert_eq!(host.object(ObjectId(10)), Some(object));
    assert_eq!(host.accepted_generation(), 3);
}

#[test]
fn zero_preparation_limit_is_rejected() {
    assert!(
        RetainedHost::with_preparation_limit(
            crate::renderer::retained_tests::source(),
            crate::tests::options(),
            0
        )
        .is_err()
    );
}
