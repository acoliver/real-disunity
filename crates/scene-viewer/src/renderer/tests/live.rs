use std::{cell::RefCell, rc::Rc, sync::mpsc, time::Duration};

use super::*;
use crate::{
    LiveDriver, LiveFrame, LiveInputEvent, LiveOptions, LiveReceipt, LiveTick, app::Content,
    live::Session,
};

#[derive(Default)]
struct Records {
    initialized: usize,
    ticks: Vec<LiveTick>,
    receipts: Vec<LiveReceipt>,
}

struct Driver(Rc<RefCell<Records>>);

impl LiveDriver for Driver {
    fn initialize(&mut self, _: Viewport) -> Result<(), String> {
        self.0.borrow_mut().initialized += 1;
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        self.0.borrow_mut().ticks.push(tick.clone());
        let view = SceneView::orthographic(
            worldrunner_render_scene::CameraPose {
                eye_millimeters: [0, -10_000, 0],
                target_millimeters: [0; 3],
            },
            worldrunner_render_scene::OrthographicLens {
                vertical_half_extent_meters: 2.0,
                near_meters: 0.1,
                far_meters: 100.0,
            },
            tick.viewport,
        )
        .unwrap();
        Ok(LiveFrame {
            view,
            quads: vec![],
            textures: None,
            clear: if tick.frame == 0 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            },
            capture: Some(if tick.frame == 0 { 100 } else { 909 }),
            exit: false,
        })
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        self.0.borrow_mut().receipts.push(receipt);
        Ok(())
    }
}

pub(super) fn pending_survives_resize_and_surface_recovery(
    event_loop: &mut EventLoop<()>,
    window: Arc<Window>,
) {
    let mut renderer = pollster::block_on(Renderer::from_source(
        event_loop.owned_display_handle(),
        window,
        SceneSource::Live,
    ))
    .unwrap();
    let records = Rc::new(RefCell::new(Records::default()));
    let mut driver = Driver(records.clone());
    let mut options = LiveOptions::new("pending frame", [64, 64]);
    options.fixed_dt = Some(Duration::from_millis(20));
    options.max_frames = Some(2);
    let (sender, receiver) = mpsc::channel();
    options.input = Some(receiver);
    let mut session = Session::new(&mut driver, options);
    skip(
        &mut session,
        &mut renderer,
        wgpu::CurrentSurfaceTexture::Timeout,
    );
    sender.send(LiveInputEvent::CaptureRequested(909)).unwrap();
    renderer.resize([0, 0]).unwrap();
    assert!(!session.redraw(&mut renderer).unwrap());
    renderer = resize_native(event_loop, renderer);
    session.event(
        &WindowEvent::Resized(winit::dpi::PhysicalSize::new(96, 64)),
        renderer.viewport,
    );
    for acquired in [
        wgpu::CurrentSurfaceTexture::Outdated,
        wgpu::CurrentSurfaceTexture::Lost,
        wgpu::CurrentSurfaceTexture::Occluded,
    ] {
        skip(&mut session, &mut renderer, acquired);
        assert_eq!(records.borrow().ticks.len(), 1);
        assert!(records.borrow().receipts.is_empty());
    }
    finish_frame(&mut session, &mut renderer, &records, 1, false);
    finish_frame(&mut session, &mut renderer, &records, 2, true);
    verify(&records.borrow());
}

fn resize_native(event_loop: &mut EventLoop<()>, renderer: Renderer) -> Renderer {
    let _ = renderer
        .window
        .as_ref()
        .unwrap()
        .request_inner_size(winit::dpi::PhysicalSize::new(96, 64));
    let mut harness = Harness {
        renderer: Some(renderer),
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        harness.renderer.as_ref().unwrap().poll().unwrap();
        event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut harness);
        if harness.renderer.as_ref().unwrap().viewport
            == Some(Viewport {
                width: 96,
                height: 64,
            })
        {
            return harness.renderer.take().unwrap();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "pending-frame native resize timed out"
        );
    }
}

fn skip(session: &mut Session<'_>, renderer: &mut Renderer, acquired: wgpu::CurrentSurfaceTexture) {
    renderer
        .execution
        .surface_mut()
        .acquisitions
        .push_back(acquired);
    assert!(!session.redraw(renderer).unwrap());
}

fn finish_frame(
    session: &mut Session<'_>,
    renderer: &mut Renderer,
    records: &Rc<RefCell<Records>>,
    count: usize,
    exit: bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        renderer.poll().unwrap();
        let finished = session.redraw(renderer).unwrap();
        if records.borrow().receipts.len() == count {
            assert_eq!(finished, exit);
            return;
        }
        assert!(!finished);
        assert!(
            std::time::Instant::now() < deadline,
            "live submission timed out"
        );
        std::thread::yield_now();
    }
}

fn verify(records: &Records) {
    assert_eq!(records.initialized, 1);
    assert_eq!(records.ticks.len(), 2);
    assert_eq!(records.receipts.len(), 2);
    assert!(
        !records.ticks[0]
            .input
            .contains(&LiveInputEvent::CaptureRequested(909))
    );
    assert!(
        records.ticks[1]
            .input
            .contains(&LiveInputEvent::CaptureRequested(909))
    );
    assert!(
        records.ticks[1]
            .input
            .contains(&LiveInputEvent::Resized([96, 64]))
    );
    assert_eq!(
        records.ticks[1].viewport,
        Viewport {
            width: 96,
            height: 64
        }
    );
    for (index, (tick, receipt)) in records.ticks.iter().zip(&records.receipts).enumerate() {
        verify_receipt(index, tick, receipt);
    }
}

fn verify_receipt(index: usize, tick: &LiveTick, receipt: &LiveReceipt) {
    assert_eq!(tick.frame, u64::try_from(index).unwrap());
    assert_eq!(receipt.frame, tick.frame);
    assert_eq!(receipt.elapsed, tick.elapsed);
    assert_eq!(tick.dt, Duration::from_millis(20));
    assert_eq!(tick.elapsed, Duration::from_millis(20 * tick.frame));
    let capture = receipt.capture.as_ref().unwrap();
    assert_eq!(capture.id, if index == 0 { 100 } else { 909 });
    assert_eq!(capture.image.viewport, tick.viewport);
    let expected = if index == 0 {
        [255, 0, 0, 255]
    } else {
        [0, 255, 0, 255]
    };
    assert!(
        capture
            .image
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel == expected)
    );
}
