use super::*;
use crate::{RetainedCaptureOutcome, RetainedEvent, RetainedHost};

pub(super) fn retained_depth_fitting(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let scene = super::super::sdsm_tests::fitted_scene();
    let shadows = Some(super::super::retained_tests::shadow_settings());
    let mut renderer = pollster::block_on(Renderer::new(
        event_loop.owned_display_handle(),
        window.clone(),
        &scene,
        shadows,
        crate::FrameProfile::Legacy,
        2,
    ))
    .unwrap();
    let size = renderer.viewport.unwrap();
    let mut host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            shadows,
            size: [size.width, size.height],
            ..crate::tests::options()
        },
    )
    .unwrap();
    present_retained(&mut renderer, &mut host);
    assert_eq!(last_fit(&renderer), None);
    present_retained(&mut renderer, &mut host);
    assert_eq!(last_fit(&renderer), None);
    // A capture must ignore eligibility delay and identify its actual host submission.
    let id = host.request_capture(host.ready_generation()).unwrap();
    present_retained(&mut renderer, &mut host);
    let fit = last_fit(&renderer).unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (2, 2));
    assert_capture(&mut renderer, &mut host, id, fit.source_frame);
    renderer.drain().unwrap();
}

fn last_fit(renderer: &Renderer) -> Option<worldrunner_render_scene::shadows::sdsm::AdoptedFit> {
    let Resident::Shadowed(scene) = &renderer.resident else {
        panic!("shadow resident");
    };
    scene.last_depth_fit()
}

fn assert_capture(renderer: &mut Renderer, host: &mut RetainedHost, id: u64, source_frame: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let event = loop {
        renderer.poll().unwrap();
        renderer.collect_retained(host).unwrap();
        if let Some(event) = host.captures.take_event() {
            break event;
        }
        assert!(
            Instant::now() < deadline,
            "SDSM capture completion timed out"
        );
        std::thread::yield_now();
    };
    let RetainedEvent::Capture {
        id: received,
        outcome,
        ..
    } = event
    else {
        panic!("capture event");
    };
    assert_eq!(received, id);
    let RetainedCaptureOutcome::Completed { frame, image } = outcome else {
        panic!("capture cancelled");
    };
    assert_eq!(frame, source_frame);
    assert_eq!(image.viewport, renderer.viewport.unwrap());
    assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 0));
}
