use super::*;

#[test]
fn dependency_validation_uses_the_requested_generations_page_policy() {
    let mut host = paged_host().with_capture_limit(1).unwrap();
    host.apply(RetainedUpdate::Terrain(None)).unwrap();
    assert_eq!(
        host.request_capture_with_pages(1),
        Err(crate::CaptureRequestError::PagingDisabled { generation: 1 })
    );
    assert_eq!(
        host.request_capture_with_pages(0),
        Err(crate::CaptureRequestError::BackPressure { limit: 1 })
    );
    assert!(host.request_capture(1).is_ok());
}

#[test]
fn already_resident_generation_dependency_captures_its_first_frame() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = paged_host().windowless(&mut driver).unwrap();
    let warm = run.host_mut().request_capture_with_pages(0).unwrap();
    finish(&mut run, &events, warm);
    let next = run.session.frame;
    jump(&mut run, true);
    let id = run.host_mut().request_capture(1).unwrap();
    assert_eq!(finish(&mut run, &events, id).0, next);
    assert_eq!(run.host().ready_generation(), 1);
    run.shutdown().unwrap();
}

fn paged_host() -> RetainedHost {
    let source = layered_scene();
    let mut parts = policy().settings().parts();
    parts.maximum_depth = 1;
    parts.maximum_selected_pages = 4;
    parts.split_threshold_millipixels = 2;
    parts.merge_threshold_millipixels = 1;
    let settings = TerrainPageSettings::new(parts).unwrap();
    parts.upload_bytes_per_frame = settings.page_bytes() * 3 / 2;
    let policy =
        TerrainPagePolicy::new(TerrainPageSettings::new(parts).unwrap(), 5, 10_000_000).unwrap();
    let terrain = source
        .terrain()
        .unwrap()
        .clone()
        .with_page_policy(Some(policy))
        .unwrap();
    host(source.with_terrain(Some(Arc::new(terrain))).unwrap())
}

fn jump(run: &mut WindowlessRetained<'_>, dependency: bool) {
    let update = RetainedUpdate::Camera {
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    };
    if dependency {
        run.host_mut().apply_with_pages(update).unwrap();
    } else {
        run.host_mut().apply(update).unwrap();
    }
}

fn completed(events: &RefCell<Vec<RetainedEvent>>, id: u64) -> Option<(u64, RgbaImage)> {
    events.borrow().iter().find_map(|event| match event {
        RetainedEvent::Capture {
            id: actual,
            outcome: RetainedCaptureOutcome::Completed { frame, image, .. },
            ..
        } if *actual == id => Some((*frame, image.clone())),
        _ => None,
    })
}

fn finish(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
    id: u64,
) -> (u64, RgbaImage) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let first_ready = pages_ready(run).then_some(run.session.frame);
        run.frame().unwrap();
        run.renderer.drain().unwrap();
        run.poll().unwrap();
        if let Some(result) = completed(events, id) {
            assert_eq!(
                Some(result.0),
                first_ready,
                "capture missed the first resident frame"
            );
            return result;
        }
        assert!(first_ready.is_none(), "resident frame was not captured");
        assert!(Instant::now() < deadline, "page capture did not complete");
    }
}

fn pages_ready(run: &mut WindowlessRetained<'_>) -> bool {
    let view = run.host().view().unwrap().unwrap();
    run.renderer.view_pages_ready(&view).unwrap()
}

#[test]
fn camera_jump_page_capture_waits_and_matches_first_resident_and_warm_frames() {
    for generation_dependency in [false, true] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = paged_host();
        host.readiness.camera.0 = worldrunner_render_scene::CameraPose {
            eye_millimeters: [100_000, 100_000, 2000],
            target_millimeters: [100_000, 102_000, 0],
        };
        let mut run = host.windowless(&mut driver).unwrap();
        run.frame().unwrap();
        run.renderer.drain().unwrap();
        run.poll().unwrap();
        let before_jump = run.session.frame;
        jump(&mut run, generation_dependency);
        let id = if generation_dependency {
            run.host_mut().request_capture(1).unwrap()
        } else {
            run.host_mut().request_capture_with_pages(1).unwrap()
        };
        assert!(run.frame().unwrap());
        run.renderer.drain().unwrap();
        run.poll().unwrap();
        assert!(completed(&events, id).is_none());
        assert_eq!(run.host().captures.waiting.len(), 1);
        assert_eq!(
            run.host().ready_generation(),
            u64::from(!generation_dependency)
        );
        let (frame, image) = finish(&mut run, &events, id);
        assert!(frame > before_jump);
        assert_eq!(run.host().ready_generation(), 1);
        let next_frame = run.session.frame;
        let warm = run.host_mut().request_capture_with_pages(1).unwrap();
        let (warm_frame, warm_image) = finish(&mut run, &events, warm);
        assert_eq!(warm_frame, next_frame);
        assert_eq!(warm_image, image);
        run.shutdown().unwrap();
    }
}

#[test]
fn waiting_page_dependency_preserves_bounds_and_shutdown_cancellation() {
    for drop_session in [false, true] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = paged_host().with_capture_limit(1).unwrap();
        host.readiness.limit = 1;
        let mut run = host.windowless(&mut driver).unwrap();
        jump(&mut run, true);
        let id = run.host_mut().request_capture_with_pages(1).unwrap();
        run.frame().unwrap();
        assert!(run.host().readiness.page_wait);
        assert_eq!(run.host().preparations_in_flight(), 1);
        assert!(matches!(
            run.host_mut().request_capture_with_pages(1),
            Err(crate::CaptureRequestError::BackPressure { limit: 1 })
        ));
        assert!(matches!(
            run.host_mut().apply(RetainedUpdate::Camera {
                pose: crate::tests::options().pose,
                lens: crate::tests::options().camera
            }),
            Err(ViewerError::BackPressure { limit: 1 })
        ));
        if !drop_session {
            run.host_mut().request_exit();
            assert!(!run.frame().unwrap());
        }
        drop(run);
        assert!(events.borrow().iter().any(|event| matches!(event, RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. } if *actual == id)));
        assert!(completed(&events, id).is_none());
    }
}

#[test]
fn disabled_paging_rejects_dependencies_before_accepting_work() {
    let mut host = host(crate::renderer::retained_tests::source())
        .with_capture_limit(1)
        .unwrap();
    assert_eq!(
        host.request_capture_with_pages(0),
        Err(crate::CaptureRequestError::PagingDisabled { generation: 0 })
    );
    assert!(
        host.apply_with_pages(RetainedUpdate::Camera {
            pose: crate::tests::options().pose,
            lens: crate::tests::options().camera
        })
        .is_err()
    );
    assert_eq!(host.accepted_generation(), 0);
    assert!(host.request_capture(0).is_ok());
    host.request_exit();
    assert_eq!(
        host.request_capture_with_pages(0),
        Err(crate::CaptureRequestError::Exiting)
    );
}
