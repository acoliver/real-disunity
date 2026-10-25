use super::*;
use crate::live::{LiveClock, startup::Startup};

struct Prewarmed {
    calls: Vec<&'static str>,
}

impl LiveDriver for Prewarmed {
    fn clock(&self) -> LiveClock {
        LiveClock::ReadyPrewarmed
    }
    fn initialize(&mut self, _: Viewport) -> Result<(), String> {
        self.calls.push("initialize");
        Ok(())
    }
    fn ready(&mut self) -> Result<(), String> {
        self.calls.push("ready");
        Ok(())
    }
    fn update(&mut self, _: &LiveTick) -> Result<LiveFrame, String> {
        self.calls.push("update");
        Err("update sentinel".into())
    }
}

fn viewport() -> Viewport {
    Viewport {
        width: 64,
        height: 64,
    }
}

fn presented(driver: &mut Prewarmed) -> Session<'_> {
    let mut session = Session::new(driver, LiveOptions::new("startup test", [64, 64]));
    session.initialized = true;
    session.startup = Startup::Presented(viewport());
    session
}

#[test]
fn single_origin_after_startup_gives_exact_first_deltas_and_cutoffs() {
    let mut driver = Prewarmed { calls: vec![] };
    let mut session = presented(&mut driver);
    let ready = Instant::now() + Duration::from_secs(1);
    session
        .commit_origin(viewport(), viewport(), ready)
        .unwrap();
    assert_eq!(session.startup, Startup::Running);
    session.input.push(
        LiveInputEvent::Focused(true),
        ready + Duration::from_nanos(5),
    );
    let first = session
        .tick_at(viewport(), ready + Duration::from_nanos(7))
        .unwrap();
    assert_eq!(first.dt, Duration::from_nanos(7));
    session.elapsed += first.dt;
    let second = session
        .tick_at(viewport(), ready + Duration::from_nanos(23))
        .unwrap();
    assert_eq!(second.dt, Duration::from_nanos(16));
    assert_eq!(second.elapsed + second.dt, Duration::from_nanos(23));
    assert_eq!(first.input, [LiveInputEvent::Focused(true)]);
    assert_eq!(driver.calls, ["ready"]);
}

#[test]
fn key_or_text_acquired_before_origin_rejects_startup_without_starting_the_clock() {
    for event in [
        LiveInputEvent::Key {
            key: "w".into(),
            pressed: true,
            repeat: false,
        },
        LiveInputEvent::Text("w".into()),
    ] {
        let mut driver = Prewarmed { calls: vec![] };
        let mut session = presented(&mut driver);
        let ready = Instant::now();
        session.input.push(event, ready);
        let error = session
            .commit_origin(viewport(), viewport(), ready)
            .unwrap_err();
        assert!(
            matches!(error, ViewerError::Driver(message) if message.contains("before the ready origin"))
        );
        assert!(session.previous.is_none());
        assert_ne!(session.startup, Startup::Running);
        assert!(driver.calls.is_empty());
    }
}

#[test]
fn focus_before_origin_is_preserved_and_clamped_to_zero() {
    let mut driver = Prewarmed { calls: vec![] };
    let mut session = presented(&mut driver);
    let ready = Instant::now() + Duration::from_millis(10);
    session.input.push(
        LiveInputEvent::Focused(false),
        ready.checked_sub(Duration::from_millis(5)).unwrap(),
    );
    session
        .commit_origin(viewport(), viewport(), ready)
        .unwrap();
    let tick = session
        .tick_at(viewport(), ready + Duration::from_nanos(1))
        .unwrap();
    assert_eq!(tick.input, [LiveInputEvent::Focused(false)]);
    assert_eq!(tick.dt, Duration::from_nanos(1));
}

#[test]
fn surface_extent_change_after_presentation_fails_closed() {
    let mut driver = Prewarmed { calls: vec![] };
    let mut session = presented(&mut driver);
    let resized = Viewport {
        width: 32,
        height: 64,
    };
    let error = session
        .commit_origin(viewport(), resized, Instant::now())
        .unwrap_err();
    assert!(matches!(error, ViewerError::Driver(message) if message.contains("extent changed")));
    assert!(session.previous.is_none());
    assert!(driver.calls.is_empty());
}

#[test]
fn missing_initial_scene_rejects_after_one_initialize_without_an_origin() {
    let mut driver = Prewarmed { calls: vec![] };
    let mut session = Session::new(&mut driver, LiveOptions::new("startup test", [64, 64]));
    let Err(missing) = session.initial_frame(viewport()) else {
        panic!("startup without an initial scene must reject");
    };
    assert!(
        matches!(missing, ViewerError::Driver(message) if message.contains("no initial scene"))
    );
    assert!(session.initialized);
    assert!(session.previous.is_none());
    assert_eq!(driver.calls, ["initialize"]);
}
