use super::*;
use std::{sync::Arc, time::Duration};

#[test]
fn streaming_terrain_rejects_retained_edits_and_capture_dependencies() {
    let source = crate::renderer::retained_tests::terrain_source();
    let terrain = Arc::new(
        source
            .terrain()
            .unwrap()
            .clone()
            .with_streaming_policy(Some(
                worldrunner_render_plan::TerrainStreamingPolicy::new(8, 9, 4, 2000).unwrap(),
            )),
    );
    let scene = source.with_terrain(Some(terrain)).unwrap();
    let mut host = RetainedHost::new(scene, crate::tests::options()).unwrap();
    assert!(matches!(
        host.apply(RetainedUpdate::Terrain(None)),
        Err(ViewerError::UnsupportedScene(
            "streaming terrain retained edits"
        ))
    ));
    assert_eq!(
        host.request_capture(0),
        Err(CaptureRequestError::StreamingTerrain)
    );
    assert_eq!(
        host.request_capture_with_pages(0),
        Err(CaptureRequestError::StreamingTerrain)
    );
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.preparations_in_flight(), 0);
}

fn host(limit: usize) -> RetainedHost {
    RetainedHost::with_preparation_limit(
        crate::renderer::retained_tests::terrain_source(),
        crate::tests::options(),
        limit,
    )
    .unwrap()
}

#[test]
fn unrelated_edits_and_resource_replacement_share_unchanged_terrain() {
    let mut host = host(4);
    let original = host.scene.clone();
    host.apply(RetainedUpdate::Replace {
        id: ObjectId(0),
        object: host.object(ObjectId(0)).unwrap(),
    })
    .unwrap();
    assert!(std::ptr::eq(
        original.terrain().unwrap(),
        host.scene.terrain().unwrap()
    ));
    host.apply(RetainedUpdate::Resources {
        resources: host.scene.resources().clone(),
        meshes: host.scene.geometry().meshes().to_vec(),
    })
    .unwrap();
    assert!(std::ptr::eq(
        original.terrain().unwrap(),
        host.scene.terrain().unwrap()
    ));
    let prepared = host.readiness.queue[1]
        .scene
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert!(std::ptr::eq(
        original.terrain().unwrap(),
        prepared.scene().terrain().unwrap()
    ));
}

#[test]
fn terrain_preparations_keep_each_atomic_generation_and_obey_pressure() {
    let mut host = host(3);
    let first = Arc::new(host.scene.terrain().unwrap().clone());
    let second = Arc::new(first.as_ref().clone());
    host.apply(RetainedUpdate::Terrain(Some(first.clone())))
        .unwrap();
    host.apply(RetainedUpdate::Terrain(Some(second.clone())))
        .unwrap();
    host.apply(RetainedUpdate::Terrain(None)).unwrap();
    assert!(host.scene.terrain().is_none());
    assert_eq!(host.ready_generation(), 0);
    assert!(matches!(
        host.apply(RetainedUpdate::Terrain(Some(first.clone()))),
        Err(ViewerError::BackPressure { limit: 3 })
    ));
    for (index, expected) in [Some(first), Some(second), None].iter().enumerate() {
        let prepared = host
            .readiness
            .queue
            .front()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        match expected {
            Some(terrain) => assert!(std::ptr::eq(
                terrain.as_ref(),
                prepared.scene().terrain().unwrap()
            )),
            None => assert!(prepared.scene().terrain().is_none()),
        }
        host.readiness.publish_front();
        assert_eq!(host.ready_generation(), index as u64 + 1);
    }
}

#[test]
fn invalid_terrain_references_leave_accepted_state_unchanged() {
    let mut host = RetainedHost::new(
        RenderScene::new(worldrunner_render_plan::SceneParts {
            resources: worldrunner_render_plan::SceneResources::new(vec![], vec![]).unwrap(),
            geometry: worldrunner_render_plan::SceneGeometry::new(vec![], vec![]).unwrap(),
            terrain: None,
            effects: None,
        })
        .unwrap(),
        crate::tests::options(),
    )
    .unwrap();
    let terrain = crate::renderer::retained_tests::terrain_source()
        .terrain()
        .unwrap()
        .clone();
    assert!(
        host.apply(RetainedUpdate::Terrain(Some(Arc::new(terrain))))
            .is_err()
    );
    assert_eq!(host.accepted_generation(), 0);
    assert_eq!(host.preparations_in_flight(), 0);
    assert!(host.scene.terrain().is_none());
}
