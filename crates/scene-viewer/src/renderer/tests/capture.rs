pub(super) fn retained_page_capture_present(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let scene = crate::retained::layered_scene();
    let mut renderer = pollster::block_on(Renderer::new(
        event_loop.owned_display_handle(),
        window.clone(),
        &scene,
        None,
        crate::FrameProfile::Legacy,
        2,
    ))
    .unwrap();
    let size = renderer.viewport.unwrap();
    let mut host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [size.width, size.height],
            ..crate::tests::options()
        },
    )
    .unwrap()
    .with_capture_limit(1)
    .unwrap();
    host.apply_with_pages(RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    })
    .unwrap();
    let id = host.request_capture_with_pages(1).unwrap();
    present_retained(&mut renderer, &mut host);
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(host.captures.waiting.len(), 1);
    assert!(matches!(
        host.request_capture_with_pages(1),
        Err(crate::CaptureRequestError::BackPressure { limit: 1 })
    ));
    let cold = page_result(&mut renderer, &mut host, id);
    let next = renderer.streaming.captures.next_frame();
    let id = host.request_capture_with_pages(1).unwrap();
    let warm = page_result(&mut renderer, &mut host, id);
    assert_eq!(warm.0, next);
    assert_eq!(cold.1, warm.1);
    cancel_waiting_pages(&mut renderer, host);
    renderer.drain().unwrap();
}

fn cancel_waiting_pages(renderer: &mut Renderer, mut host: RetainedHost) {
    renderer
        .resident
        .scene_mut()
        .invalidate_terrain_pages([0.0; 2], [1.0; 2]);
    let id = host.request_capture_with_pages(1).unwrap();
    present_retained(renderer, &mut host);
    assert_eq!(host.captures.waiting.len(), 1);
    let cancelled = Rc::new(RefCell::new(None));
    let mut consumer = Consumer(cancelled.clone());
    let mut session = Session::new(host, &mut consumer);
    session.shutdown().unwrap();
    assert!(
        matches!(*cancelled.borrow(), Some(RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. }) if actual == id)
    );
}

fn page_result(
    renderer: &mut Renderer,
    host: &mut RetainedHost,
    id: u64,
) -> (u64, worldrunner_render_scene::offscreen::RgbaImage) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        renderer.poll().unwrap();
        renderer.collect_retained(host).unwrap();
        if let Some(RetainedEvent::Capture {
            id: actual,
            outcome: RetainedCaptureOutcome::Completed { frame, image, .. },
            ..
        }) = host.captures.take_event()
        {
            assert_eq!(actual, id);
            return (frame, image);
        }
        renderer.render_retained(host).unwrap();
        assert!(Instant::now() < deadline, "page capture timed out");
        std::thread::yield_now();
    }
}

use super::*;
use crate::{
    RetainedCaptureOutcome, RetainedDriver, RetainedEvent, RetainedHost, RetainedTick,
    RetainedUpdate, app::Content, retained::Session,
};
use std::{cell::RefCell, rc::Rc};

struct Consumer(Rc<RefCell<Option<RetainedEvent>>>);
impl RetainedDriver for Consumer {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        panic!("completion polling must not tick the simulation");
    }
    fn event(
        &mut self,
        host: &mut crate::RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        assert!(!matches!(event, RetainedEvent::Presented { .. }));
        if matches!(
            event,
            RetainedEvent::Input(crate::RetainedInput::Occluded(true))
        ) {
            host.set_cursor_grab(crate::CursorGrabMode::None)
                .map_err(|error| error.to_string())?;
            host.set_cursor_visible(true)
                .map_err(|error| error.to_string())?;
        }
        if matches!(event, RetainedEvent::Capture { .. }) {
            *self.0.borrow_mut() = Some(event.clone());
            host.request_exit();
        }
        Ok(())
    }
}

pub(super) fn retained_capture_present(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let scene = super::super::retained_tests::source();
    let mut renderer = pollster::block_on(Renderer::new(
        event_loop.owned_display_handle(),
        window.clone(),
        &scene,
        None,
        crate::FrameProfile::Legacy,
        2,
    ))
    .unwrap();
    let size = renderer.viewport.unwrap();
    let mut host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [size.width, size.height],
            ..crate::tests::options()
        },
    )
    .unwrap()
    .with_capture_limit(1)
    .unwrap();
    host.apply(RetainedUpdate::Camera {
        convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        pose: crate::tests::options().pose,
        lens: crate::tests::options().camera,
    })
    .unwrap();
    let id = host.request_capture(1).unwrap();
    assert!(matches!(
        host.request_capture(1),
        Err(crate::CaptureRequestError::BackPressure { limit: 1 })
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        renderer.poll().unwrap();
        if renderer.render_retained(&mut host).unwrap() {
            break;
        }
        assert!(Instant::now() < deadline, "submission timed out");
        std::thread::yield_now();
    }
    let event = collect_without_redraw(&mut renderer, host);
    assert_capture(event, id, size);
    renderer.drain().unwrap();
}

fn collect_without_redraw(renderer: &mut Renderer, host: RetainedHost) -> RetainedEvent {
    let completed = Rc::new(RefCell::new(None));
    let mut consumer = Consumer(completed.clone());
    let mut session = Session::new(host, &mut consumer);
    session.window_ready(renderer.window.as_ref().unwrap().clone());
    renderer.resize([0, 0]).unwrap();
    session.surface_size([0, 0]);
    session.event(&winit::event::WindowEvent::Occluded(true), None);
    assert!(session.take_failure().is_none());
    let deadline = Instant::now() + Duration::from_secs(10);
    while completed.borrow().is_none() {
        renderer.poll().unwrap();
        session.poll(renderer).unwrap();
        assert!(
            Instant::now() < deadline,
            "capture delivery timed out without redraw"
        );
        std::thread::yield_now();
    }
    assert!(renderer.viewport.is_none());
    assert!(session.exit_requested());
    completed.borrow_mut().take().unwrap()
}

fn assert_capture(event: RetainedEvent, id: u64, size: worldrunner_render_scene::Viewport) {
    let RetainedEvent::Capture {
        id: received,
        generation,
        outcome,
    } = event
    else {
        panic!("missing capture event");
    };
    assert_eq!((received, generation), (id, 1));
    let RetainedCaptureOutcome::Completed { frame, image, .. } = outcome else {
        panic!("capture cancelled");
    };
    assert_eq!(frame, 0);
    assert_eq!(image.viewport, size);
    let offset = ((size.height / 2 * size.width + size.width / 2) * 4) as usize;
    assert_eq!(&image.pixels[offset..offset + 4], [255, 255, 255, 255]);
}
