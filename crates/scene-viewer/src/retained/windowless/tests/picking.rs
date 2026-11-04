use super::*;
use crate::ObjectId;

fn pixel(host: &RetainedHost, point: glam::Vec3) -> [f64; 2] {
    let p = host
        .view()
        .unwrap()
        .unwrap()
        .resolved_view()
        .world_to_screen(point)
        .unwrap();
    [f64::from(p.x), f64::from(p.y)]
}
fn present_generation(run: &mut WindowlessRetained<'_>, generation: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        if run
            .host()
            .statistics()
            .is_some_and(|s| s.generation == generation)
        {
            break;
        }
        assert!(Instant::now() < deadline, "presentation did not advance");
    }
}

#[test]
fn picking_tracks_presented_objects_and_not_accepted_or_renderer_published_edits() {
    let mut driver = Driver::default();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let center = pixel(run.host(), glam::Vec3::ZERO);
    assert!(run.host().pick(center, |_| true).is_none());
    present_generation(&mut run, 0);
    assert_eq!(run.host().pick(center, |_| true).unwrap().generation, 0);
    let object = run.host().object(ObjectId(0)).unwrap();
    let mut transform = *object.transform();
    transform.position_millimeters[0] = 1500;
    let moved = object.with_transform(transform);
    run.host_mut()
        .apply(RetainedUpdate::Replace {
            id: ObjectId(0),
            object: moved,
        })
        .unwrap();
    assert_eq!(run.host().accepted_generation(), 1);
    assert_eq!(run.host().pick(center, |_| true).unwrap().generation, 0);
    // Publishing to renderer residency alone must not replace the displayed snapshot.
    run.renderer
        .publish_retained(&mut run.session.host)
        .unwrap();
    assert_eq!(run.host().ready_generation(), 1);
    assert_eq!(run.host().pick(center, |_| true).unwrap().generation, 0);
    present_generation(&mut run, 1);
    assert!(run.host().pick(center, |_| true).is_none());
    let moved_pixel = pixel(run.host(), glam::Vec3::new(1.5, 0.0, 0.0));
    assert_eq!(
        run.host().pick(moved_pixel, |_| true).unwrap().generation,
        1
    );
    run.host_mut()
        .apply(RetainedUpdate::Remove(ObjectId(0)))
        .unwrap();
    assert_eq!(
        run.host().pick(moved_pixel, |_| true).unwrap().generation,
        1
    );
    present_generation(&mut run, 2);
    assert!(run.host().pick(moved_pixel, |_| true).is_none());
    run.shutdown().unwrap();
}

#[test]
fn picking_camera_resources_and_physical_resize_wait_for_presentation() {
    let mut driver = Driver::default();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    present_generation(&mut run, 0);
    let center = [32.0; 2];
    let options = run.host().options().clone();
    let pose = worldrunner_render_scene::CameraPose {
        eye_millimeters: [4000, -2000, 2000],
        target_millimeters: [4000, 0, 0],
    };
    run.host_mut()
        .apply(RetainedUpdate::Camera {
            pose,
            lens: options.camera,
            convention: options.camera_convention,
        })
        .unwrap();
    assert!(run.host().pick(center, |_| true).is_some());
    present_generation(&mut run, 1);
    assert!(run.host().pick(center, |_| true).is_none());
    run.host_mut()
        .apply(RetainedUpdate::Camera {
            pose: options.pose,
            lens: options.camera,
            convention: options.camera_convention,
        })
        .unwrap();
    present_generation(&mut run, 2);
    run.resize([128, 64]).unwrap();
    assert!(run.host().pick(center, |_| true).is_some());
    present_generation(&mut run, 2);
    assert!(run.host().pick([64.0, 32.0], |_| true).is_some());
    assert!(run.host().pick([f64::NAN, 0.0], |_| true).is_none());
    assert!(run.host().pick([-1.0, 0.0], |_| true).is_none());
    run.shutdown().unwrap();
}

#[test]
fn event_thread_picking_supports_both_handedness_conventions() {
    struct Picker;
    impl RetainedDriver for Picker {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            Ok(())
        }
        fn event(
            &mut self,
            host: &mut crate::RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            if matches!(event, RetainedEvent::Presented { .. }) {
                assert_eq!(
                    host.pick([32.0; 2], |_| true).unwrap().hit.object,
                    ObjectId(0)
                );
                assert!(host.pick([32.0; 2], |_| false).is_none());
            }
            Ok(())
        }
    }
    for handedness in [
        worldrunner_camera::ViewHandedness::RightHanded,
        worldrunner_camera::ViewHandedness::DirectXLeftHanded,
    ] {
        let mut host = host(crate::renderer::retained_tests::source());
        host.options.camera_convention = worldrunner_camera::ViewConvention { handedness };
        host.readiness.camera.2 = host.options.camera_convention;
        let mut driver = Picker;
        let mut run = host.windowless(&mut driver).unwrap();
        present_generation(&mut run, 0);
        run.shutdown().unwrap();
    }
}

#[test]
fn picking_keeps_displayed_resources_while_preparation_is_held() {
    let mut driver = Driver::default();
    let source = crate::renderer::retained_tests::source();
    let mut run = host(source.clone()).windowless(&mut driver).unwrap();
    present_generation(&mut run, 0);
    let center = [32.0; 2];
    let (release, wait) = std::sync::mpsc::channel();
    run.host_mut().readiness.preparation_hook = Some(Box::new(move || {
        wait.recv().unwrap();
    }));
    let lod = &source.geometry().meshes()[0].lods()[0];
    let mut vertices = lod.vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[0] += 4000;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: source.resources().clone(),
            meshes: vec![mesh],
        })
        .unwrap();
    assert!(run.frame().unwrap());
    assert_eq!(run.host().accepted_generation(), 1);
    assert_eq!(run.host().ready_generation(), 0);
    assert_eq!(run.host().pick(center, |_| true).unwrap().generation, 0);
    release.send(()).unwrap();
    present_generation(&mut run, 1);
    assert!(run.host().pick(center, |_| true).is_none());
    run.shutdown().unwrap();
}

#[test]
fn windowless_accumulated_capture_publishes_picking_without_presented_event() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let recipe = crate::RetainedCaptureRecipe::new(
        2,
        2,
        crate::CaptureJitter::Halton23,
        false,
        vec![crate::CaptureOutput::Srgb8],
        4 * 1024 * 1024,
    )
    .unwrap();
    let id = run.host_mut().request_capture_recipe(0, recipe).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(!run.frame().unwrap());
        run.renderer.drain().unwrap();
        run.poll().unwrap();
        if events.borrow().iter().any(
            |event| matches!(event, RetainedEvent::Capture { id: actual, .. } if *actual == id),
        ) {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, RetainedEvent::Presented { .. }))
    );
    assert_eq!(run.host().pick([32.0; 2], |_| true).unwrap().generation, 0);
    run.shutdown().unwrap();
}
