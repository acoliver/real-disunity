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
    fn event(&mut self, event: &RetainedEvent) -> Result<(), String> {
        assert!(!matches!(event, RetainedEvent::Presented { .. }));
        if matches!(event, RetainedEvent::Capture { .. }) {
            *self.0.borrow_mut() = Some(event.clone());
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
    renderer.resize([0, 0]).unwrap();
    session.surface_size([0, 0]);
    session.event(&winit::event::WindowEvent::Occluded(true), None);
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
