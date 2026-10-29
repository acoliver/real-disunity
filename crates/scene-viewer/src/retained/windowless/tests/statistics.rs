use super::*;

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
