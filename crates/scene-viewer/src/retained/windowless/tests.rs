mod celestial;
mod convention;
mod environment;
mod errors;
mod lifecycle;
pub(crate) mod pages;
mod terrain;
mod timing;
#[test]
fn typed_hdr_recipe_completes_without_more_frames_and_shutdown_cancels_it() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut host = host(crate::renderer::retained_tests::source());
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 4,
        tone_map: crate::ToneMap::Reinhard,
    };
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::LinearHdr],
        1024 * 1024,
    )
    .unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    let id = run
        .host_mut()
        .request_capture_recipe(0, recipe.clone())
        .unwrap();
    assert!(run.frame().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.poll().unwrap();
        if events.borrow().iter().any(|event| matches!(event, RetainedEvent::Capture {
            id: actual, outcome: RetainedCaptureOutcome::RecipeCompleted { products, time, .. }, ..
        } if *actual == id && products.srgb8.is_none() && products.linear_hdr.as_ref().unwrap().pixels.len() == 4096 && *time == Duration::ZERO)) { break; }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let id = run.host_mut().request_capture_recipe(0, recipe).unwrap();
    assert!(run.frame().unwrap());
    run.shutdown().unwrap();
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, RetainedEvent::Capture {
        id: actual, outcome: RetainedCaptureOutcome::Cancelled, ..
    } if *actual == id))
    );
}

#[test]
fn wall_clock_captures_advance_without_a_fixed_override() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut host = host(crate::renderer::retained_tests::source());
    host.set_render_clock(RenderClock::Wall);
    let mut run = host.windowless(&mut driver).unwrap();
    let first = capture(&mut run, &events).0;
    let second = capture(&mut run, &events).0;
    assert!(second > first);
    run.shutdown().unwrap();
}

#[test]
fn driver_failure_cancels_captures_and_closes_windowless_execution() {
    struct Failing(Rc<RefCell<Vec<RetainedEvent>>>);
    impl RetainedDriver for Failing {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            Err("update failed".into())
        }
        fn event(
            &mut self,
            _host: &mut crate::RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            self.0.borrow_mut().push(event.clone());
            Ok(())
        }
    }
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut driver = Failing(events.clone());
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let request = run.host_mut().request_capture(0).unwrap();
    assert!(matches!(run.frame(), Err(ViewerError::Driver(message)) if message == "update failed"));
    assert!(events.borrow().iter().any(|event| matches!(event,
        RetainedEvent::Capture { id, outcome: RetainedCaptureOutcome::Cancelled, .. } if *id == request)));
    assert!(run.frame().is_err());
    assert_eq!(events.borrow().last(), Some(&RetainedEvent::Shutdown));
}

#[test]
fn fixed_clock_repeats_water_flow_and_records_the_rendered_time() {
    let base = crate::renderer::retained_tests::source();
    let mut water = crate::tests::scene(true)
        .effects()
        .unwrap()
        .water()
        .unwrap()
        .clone();
    water.east_min_millimeters = -2000;
    water.north_min_millimeters = -2000;
    water.east_extent_millimeters = 4000;
    water.north_extent_millimeters = 4000;
    water.height_millimeters = 100;
    water.flow_millimeters_per_second = [700, 300];
    let scene = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(
            base.effects()
                .unwrap()
                .clone()
                .with_water_bodies(vec![water])
                .unwrap(),
        ),
    })
    .unwrap();
    let host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host.windowless(&mut driver).unwrap();
    let first = capture(&mut run, &events);
    assert_eq!(first, capture(&mut run, &events));
    run.host_mut()
        .set_render_clock(RenderClock::Fixed(Duration::from_secs(2)));
    let changed = capture(&mut run, &events);
    assert_eq!(changed.0, Duration::from_secs(2));
    assert_ne!(first.1, changed.1);
    assert_eq!(changed, capture(&mut run, &events));
    run.shutdown().unwrap();
}

#[test]
fn dropping_windowless_cancels_submitted_and_waiting_captures() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let submitted = run.host_mut().request_capture(0).unwrap();
    assert!(run.frame().unwrap());
    let waiting = run.host_mut().request_capture(0).unwrap();
    drop(run);
    for expected in [submitted, waiting] {
        assert!(events.borrow().iter().any(|event| matches!(event,
            RetainedEvent::Capture { id, outcome: RetainedCaptureOutcome::Cancelled, .. } if *id == expected)));
    }
}

use super::*;
use crate::{RenderClock, RetainedCaptureOutcome, RetainedEvent, RetainedTick, RetainedUpdate};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use worldrunner_render_plan::*;
use worldrunner_render_scene::offscreen::RgbaImage;

#[derive(Default)]
struct Driver {
    events: Rc<RefCell<Vec<RetainedEvent>>>,
    ticks: u64,
}
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        self.ticks += 1;
        Ok(())
    }
    fn event(
        &mut self,
        _host: &mut crate::RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.events.borrow_mut().push(event.clone());
        Ok(())
    }
}
fn host(scene: RenderScene) -> RetainedHost {
    RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            ..crate::tests::options()
        },
    )
    .unwrap()
}
fn capture(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
) -> (Duration, RgbaImage) {
    let generation = run.host().accepted_generation();
    let request = run.host_mut().request_capture(generation).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        let result = events.borrow().iter().find_map(|event| match event {
            RetainedEvent::Capture {
                id,
                generation: actual,
                outcome: RetainedCaptureOutcome::Completed { time, image, .. },
            } if *id == request => {
                assert_eq!(*actual, generation);
                Some((*time, image.clone()))
            }
            _ => None,
        });
        if let Some(result) = result {
            return result;
        }
        assert!(Instant::now() < deadline, "capture timed out");
        std::thread::yield_now();
    }
}
fn windy() -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    let material = base.resources().materials()[0]
        .with_features(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: Some(
                SceneWind::new(
                    SceneDirection::new([1_000_000, 0, 0]).unwrap(),
                    500,
                    1000,
                    0,
                )
                .unwrap(),
            ),
            layer: None,
            translucency: None,
        })
        .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}
#[test]
fn fixed_clock_captures_are_identical_and_wind_changes_in_both_residents() {
    for shadows in [
        None,
        Some(crate::renderer::retained_tests::shadow_settings()),
    ] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = host(windy());
        host.options.shadows = shadows;
        let mut run = host.windowless(&mut driver).unwrap();
        let first = capture(&mut run, &events);
        assert_eq!(first.0, Duration::ZERO);
        assert_eq!(first, capture(&mut run, &events));
        run.host_mut()
            .set_render_clock(RenderClock::Fixed(Duration::from_millis(250)));
        let changed = capture(&mut run, &events);
        assert_eq!(changed.0, Duration::from_millis(250));
        assert_ne!(first.1, changed.1);
        assert_eq!(changed, capture(&mut run, &events));
        run.shutdown().unwrap();
    }
}
#[test]
fn lifecycle_resize_readiness_pressure_and_shutdown_use_the_retained_session() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let host = host(crate::renderer::retained_tests::source())
        .with_capture_limit(1)
        .unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    let object = run.host().object(crate::ObjectId(0)).unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Replace {
            id: crate::ObjectId(0),
            object,
        })
        .unwrap();
    assert_eq!(capture(&mut run, &events).1.viewport.width, 64);
    assert_eq!(run.host().ready_generation(), 1);
    let before = run.session.frame;
    for _ in 0..4 {
        run.poll().unwrap();
    }
    assert_eq!(run.session.frame, before);
    run.resize([48, 32]).unwrap();
    assert_eq!(capture(&mut run, &events).1.viewport.width, 48);
    run.resize([0, 0]).unwrap();
    assert!(!run.frame().unwrap());
    let id = run.host_mut().request_capture(1).unwrap();
    assert!(matches!(
        run.host_mut().request_capture(1),
        Err(crate::CaptureRequestError::BackPressure { limit: 1 })
    ));
    run.shutdown().unwrap();
    assert!(events.borrow().iter().any(|event| matches!(event, RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. } if *actual == id)));
    assert!(events.borrow().iter().any(|event| matches!(
        event,
        RetainedEvent::Input(crate::RetainedInput::Resized([0, 0]))
    )));
    assert!(run.frame().is_err());
}
#[test]
fn wall_clock_is_the_default_and_driver_can_override_it() {
    struct Fixed;
    impl RetainedDriver for Fixed {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            Ok(())
        }
        fn render_clock(&self) -> Option<RenderClock> {
            Some(RenderClock::Fixed(Duration::from_secs(7)))
        }
    }
    assert_eq!(RenderClock::default(), RenderClock::Wall);
    let mut driver = Fixed;
    let mut run = host(windy()).windowless(&mut driver).unwrap();
    assert!(run.frame().unwrap());
    assert_eq!(run.renderer.frame_time, Duration::from_secs(7));
    run.shutdown().unwrap();
}

#[test]
fn temporal_subframes_never_emit_presented_and_shutdown_cancels_accumulation() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let recipe = crate::RetainedCaptureRecipe::new(
        2,
        8,
        crate::CaptureJitter::Halton23,
        true,
        vec![crate::CaptureOutput::Srgb8],
        4 * 1024 * 1024,
    )
    .unwrap();
    let id = run.host_mut().request_capture_recipe(0, recipe).unwrap();
    assert!(!run.frame().unwrap());
    run.renderer.drain().unwrap();
    run.poll().unwrap();
    assert!(!run.frame().unwrap());
    assert!(
        !events
            .borrow()
            .iter()
            .any(|event| matches!(event, RetainedEvent::Presented { .. }))
    );
    run.shutdown().unwrap();
    assert!(events.borrow().iter().any(|event| matches!(event, RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. } if *actual == id)));
}

mod statistics;
