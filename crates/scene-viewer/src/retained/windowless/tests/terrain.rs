use super::*;
use std::sync::Arc;

fn replacement(base: &SceneTerrain) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: base.columns(),
        rows: base.rows(),
        origin_east_millimeters: base.origin_east_millimeters(),
        origin_north_millimeters: base.origin_north_millimeters(),
        sample_spacing_millimeters: base.sample_spacing_millimeters(),
        heights_millimeters: base.heights_millimeters().to_vec(),
        surface_indices: base.surface_indices().to_vec(),
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance {
                tint_percent: [15, 100, 15],
                ..SceneTerrainAppearance::NEUTRAL
            },
        )],
        lod_policy: base.lod_policy(),
    })
    .unwrap()
}

#[test]
fn terrain_install_replace_remove_publish_capture_pixels_in_windowless_host() {
    let source = crate::renderer::retained_tests::terrain_source();
    let terrain = Arc::new(source.terrain().unwrap().clone());
    let empty = source.with_terrain(None).unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(empty).windowless(&mut driver).unwrap();
    let absent = capture(&mut run, &events).1;
    run.host_mut()
        .apply(RetainedUpdate::Terrain(Some(terrain.clone())))
        .unwrap();
    let installed = capture(&mut run, &events).1;
    assert_eq!(run.host().ready_generation(), 1);
    assert_ne!(installed, absent);
    run.host_mut()
        .apply(RetainedUpdate::Terrain(Some(Arc::new(replacement(
            &terrain,
        )))))
        .unwrap();
    let replaced = capture(&mut run, &events).1;
    assert_eq!(run.host().ready_generation(), 2);
    assert_ne!(replaced, installed);
    run.host_mut().apply(RetainedUpdate::Terrain(None)).unwrap();
    assert_eq!(capture(&mut run, &events).1, absent);
    assert_eq!(run.host().ready_generation(), 3);
    run.shutdown().unwrap();
}

#[test]
fn terrain_publication_and_capture_wait_for_preparation_without_blocking_frames() {
    let source = crate::renderer::retained_tests::terrain_source();
    let terrain = Arc::new(source.terrain().unwrap().clone());
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(source.with_terrain(None).unwrap())
        .windowless(&mut driver)
        .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    run.host_mut().readiness.preparation_hook = Some(Box::new(move || receive.recv().unwrap()));
    run.host_mut()
        .apply(RetainedUpdate::Terrain(Some(terrain)))
        .unwrap();
    let request = run.host_mut().request_capture(1).unwrap();
    for _ in 0..4 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert_eq!(run.host().ready_generation(), 0);
    }
    assert!(!events.borrow().iter().any(|event| matches!(event,
        RetainedEvent::Capture { id, .. } if *id == request)));
    send.send(()).unwrap();
    let image = capture(&mut run, &events).1;
    assert_eq!(run.host().ready_generation(), 1);
    assert!(events.borrow().iter().any(|event| matches!(event,
        RetainedEvent::Capture { id, generation: 1, outcome: RetainedCaptureOutcome::Completed { image: actual, .. } }
        if *id == request && *actual == image)));
    run.shutdown().unwrap();
}
