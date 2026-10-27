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
    .with_streaming_policy(base.streaming_policy())
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

fn streamed_source() -> RenderScene {
    let source = crate::renderer::retained_tests::terrain_source();
    let base = source.terrain().unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 3,
        rows: 3,
        origin_east_millimeters: -1000,
        origin_north_millimeters: -1000,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![-300; 9],
        surface_indices: vec![0; 9],
        surfaces: base.surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    source.with_terrain(Some(Arc::new(terrain))).unwrap()
}

#[test]
fn geometry_capture_and_generation_wait_for_submitted_residency() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(streamed_source()).windowless(&mut driver).unwrap();
    let options = crate::tests::options();
    run.host_mut()
        .apply_with_geometry(RetainedUpdate::Camera {
            pose: options.pose,
            lens: options.camera,
        })
        .unwrap();
    let id = run.host_mut().request_capture_with_geometry(1).unwrap();
    run.frame().unwrap();
    assert_eq!(run.host().ready_generation(), 0);
    assert!(run.host().captures.waiting.iter().any(|r| r.0 == id));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !events.borrow().iter().any(|event| {
        matches!(event, RetainedEvent::Capture {
        id: actual, outcome: RetainedCaptureOutcome::Completed { .. }, ..
    } if *actual == id)
    }) {
        assert!(Instant::now() < deadline);
        run.frame().unwrap();
        run.poll().unwrap();
        std::thread::yield_now();
    }
    assert_eq!(run.host().ready_generation(), 1);
    run.shutdown().unwrap();
}

#[test]
fn streamed_terrain_install_replace_remove_publish_atomic_pixels() {
    let source = streamed_source();
    let terrain = Arc::new(source.terrain().unwrap().clone());
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(source.with_terrain(None).unwrap())
        .windowless(&mut driver)
        .unwrap();
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
    assert_eq!(run.host().ready_generation(), 1);
    assert_ne!(capture(&mut run, &events).1, installed);
    assert_eq!(run.host().ready_generation(), 2);
    run.host_mut().apply(RetainedUpdate::Terrain(None)).unwrap();
    assert_eq!(capture(&mut run, &events).1, absent);
    assert_eq!(run.host().ready_generation(), 3);
    run.shutdown().unwrap();
}
