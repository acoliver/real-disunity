use super::*;

#[test]
fn geometry_capture_observations_freeze_source_submission() {
    let base = crate::renderer::retained_tests::terrain_source();
    let terrain = base.terrain().unwrap().clone().with_streaming_policy(Some(
        worldrunner_render_plan::TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap(),
    ));
    let source = base
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(source).windowless(&mut driver).unwrap();
    let id = run.host_mut().request_capture(0).unwrap();
    run.frame().unwrap();
    let first = *run.host().statistics().unwrap();
    let Some(crate::TerrainGeometryStatistics::Streaming(geometry)) = first.scene.terrain_geometry
    else {
        panic!("streaming source observation");
    };
    assert!(geometry.complete);
    run.host_mut().apply(RetainedUpdate::Terrain(None)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().statistics().unwrap().generation == 0 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    assert_eq!(
        run.host().statistics().unwrap().scene.terrain_geometry,
        None
    );
    run.renderer.drain().unwrap();
    run.poll().unwrap();
    let events = events.borrow();
    let outcome = events
        .iter()
        .find_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                outcome,
                ..
            } if *actual == id => Some(outcome),
            _ => None,
        })
        .unwrap();
    let RetainedCaptureOutcome::Completed {
        frame, observation, ..
    } = outcome
    else {
        panic!("completed capture");
    };
    assert_eq!(*frame, first.frame);
    assert_eq!(
        observation.scene.terrain_geometry,
        first.scene.terrain_geometry
    );
    drop(events);
    run.shutdown().unwrap();
}

#[test]
fn supplier_prestart_capture_freezes_selected_scene_statistics_and_shadows() {
    for shadows in [
        None,
        Some(crate::renderer::retained_tests::shadow_settings()),
    ] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = supplier_capture_host(shadows);
        assert!(host.statistics().is_none());
        let id = host.request_capture(0).unwrap();
        assert!(host.statistics().is_none());
        let mut run = host.windowless(&mut driver).unwrap();
        assert_eq!(run.host().options().shadows, shadows);
        assert!(run.host().statistics().is_none());
        assert!(run.frame().unwrap());
        let source = *run.host().statistics().unwrap();
        assert_eq!(source.generation, 0);
        assert_eq!(source.scene.meshes.instances, 1);
        run.renderer.drain().unwrap();
        run.poll().unwrap();
        let events = events.borrow();
        let (generation, frame, observation) = events
            .iter()
            .find_map(|event| match event {
                RetainedEvent::Capture {
                    id: actual,
                    generation,
                    outcome:
                        RetainedCaptureOutcome::Completed {
                            frame, observation, ..
                        },
                } if *actual == id => Some((*generation, *frame, observation)),
                _ => None,
            })
            .expect("queued supplier capture completed");
        assert_eq!(frame, source.frame);
        assert_eq!(generation, source.generation);
        assert_eq!(observation.generation, source.generation);
        assert_eq!(observation.scene, source.scene);
        assert_capture_shadows(observation.scene.effects.shadows, shadows.is_some());
        drop(events);
        run.shutdown().unwrap();
    }
}

fn assert_capture_shadows(observed: crate::ShadowStatistics, enabled: bool) {
    let (installation, binding) = if enabled {
        (
            crate::PassInstallation::Installed,
            crate::ReceiverBinding::Bound,
        )
    } else {
        (
            crate::PassInstallation::Absent,
            crate::ReceiverBinding::Unbound,
        )
    };
    assert_eq!(observed.installation, installation);
    assert_eq!(observed.receivers.meshes, binding);
    assert_eq!(observed.receivers.terrain, crate::ReceiverBinding::Unbound);
}

fn supplier_capture_host(
    shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
) -> RetainedHost {
    let options = crate::ViewerOptions {
        size: [64, 64],
        shadows: shadows
            .is_none()
            .then(crate::renderer::retained_tests::shadow_settings),
        render_clock: RenderClock::Fixed(Duration::ZERO),
        ..crate::tests::options()
    };
    RetainedHost::with_initial_content(options, move |identity| {
        assert!(!identity.name.is_empty());
        Ok(crate::InitialContent {
            scene: crate::renderer::retained_tests::source(),
            settings: Some(crate::FrameSettings {
                shadows,
                frame_profile: crate::FrameProfile::Hdr {
                    samples: 1,
                    tone_map: crate::ToneMap::Reinhard,
                },
                presentation_budget: crate::PresentationBudget::default().into(),
            }),
        })
    })
    .unwrap()
}

#[test]
fn capture_statistics_match_source_frame_and_survive_later_edits() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let id = run.host_mut().request_capture(0).unwrap();
    assert!(run.frame().unwrap());
    let first = *run.host().statistics().unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Remove(crate::ObjectId(0)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().statistics().unwrap().generation == 0 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    run.renderer.drain().unwrap();
    run.poll().unwrap();
    let events = events.borrow();
    let outcome = events
        .iter()
        .find_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                outcome,
                ..
            } if *actual == id => Some(outcome),
            _ => None,
        })
        .unwrap();
    let crate::RetainedCaptureOutcome::Completed {
        frame, observation, ..
    } = outcome
    else {
        panic!("completed capture");
    };
    assert_eq!(*frame, first.frame);
    assert_eq!(observation.generation, first.generation);
    assert_eq!(observation.scene, first.scene);
    assert_ne!(observation.scene, run.host().statistics().unwrap().scene);
    drop(events);
    run.shutdown().unwrap();
}

#[test]
fn statistics_follow_presented_frames_and_consume_uploads_once() {
    let mut driver = Driver::default();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    assert!(run.host().statistics().is_none());
    assert!(run.frame().unwrap());
    let first = *run.host().statistics().unwrap();
    assert_eq!(first.frame, 0);
    assert_eq!(first.generation, 0);
    assert_eq!(first.scene.meshes.calls, 1);
    assert_eq!(first.scene.meshes.instances, 1);
    assert_eq!(first.scene.textures.resident_bytes, 4);
    assert_eq!(first.scene.textures.uploaded_bytes, 4);
    run.host_mut()
        .apply(RetainedUpdate::Remove(crate::ObjectId(0)))
        .unwrap();
    assert_eq!(run.host().statistics(), Some(&first));
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().statistics().unwrap().generation == 0 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    assert_removed(run.host().statistics().unwrap(), &first);
    run.shutdown().unwrap();
}

fn assert_removed(second: &crate::RetainedFrameStatistics, first: &crate::RetainedFrameStatistics) {
    assert!(second.frame > first.frame);
    assert_eq!(second.generation, 1);
    assert_eq!(second.scene.meshes.calls, 0);
    assert_eq!(second.scene.meshes.instances, 0);
    assert_eq!(second.scene.textures.resident_bytes, 4);
    assert_eq!(second.scene.textures.uploaded_bytes, 0);
}

#[test]
fn presented_callback_reads_the_matching_frame_snapshot() {
    struct Observer;
    impl RetainedDriver for Observer {
        fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
            if tick.frame == 0 {
                assert!(host.statistics().is_none());
            }
            Ok(())
        }
        fn event(
            &mut self,
            host: &mut crate::RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            if let RetainedEvent::Presented { frame } = event {
                assert_eq!(host.statistics().unwrap().frame, *frame);
                assert_eq!(host.statistics().unwrap().scene.meshes.instances, 1);
            }
            Ok(())
        }
    }
    let mut driver = Observer;
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    assert!(run.frame().unwrap());
    run.shutdown().unwrap();
}

#[test]
fn resource_replacement_reports_new_generation_uploads() {
    let scene = crate::renderer::retained_tests::source();
    let mut driver = Driver::default();
    let mut run = host(scene.clone()).windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().statistics().unwrap().generation != 1 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    let stats = run.host().statistics().unwrap();
    assert_eq!(stats.scene.textures.uploaded_bytes, 4);
    assert_eq!(
        stats.scene.mesh_buffers.uploaded_bytes,
        stats.scene.mesh_buffers.resident_bytes
    );
    run.shutdown().unwrap();
}

#[test]
fn empty_scene_records_zero_optional_pass_draws() {
    let source = crate::renderer::retained_tests::source();
    let source = source.with_objects_and_effects(vec![], None).unwrap();
    let mut driver = Driver::default();
    let mut run = host(source).windowless(&mut driver).unwrap();
    run.frame().unwrap();
    assert_eq!(
        run.host().statistics().unwrap().scene.passes,
        worldrunner_render_scene::draw::PassDrawStatistics::default()
    );
    run.shutdown().unwrap();
}
