use super::{
    Desktop,
    tests::{present, report, request, tick},
};
use crate::playback::Playback;
use serde_json::json;
use std::{path::Path, time::Duration};
use worldrunner::scene::viewer::{
    IngressOrigin, LiveClock, LiveDriver, LiveInputEvent, LiveInputSample,
};

#[test]
fn presentation_startup_is_explicit_strict_interactive_and_exact() {
    let installation = crate::test_support::installation();
    let artifacts = crate::test_support::scratch("desktop-");
    let cases = [
        (false, None, Ok(LiveClock::Legacy)),
        (true, None, Ok(LiveClock::Acquisition)),
        (
            true,
            Some(json!("ready_prewarmed_v1")),
            Ok(LiveClock::ReadyPrewarmed),
        ),
        (
            false,
            Some(json!("ready_prewarmed_v1")),
            Err("requires a strict"),
        ),
        (true, Some(json!("ready_prewarmed_v2")), Err("unsupported")),
        (true, Some(json!(true)), Err("unsupported")),
    ];
    for (strict, startup, expected) in cases {
        let mut request = request(artifacts.path());
        if strict {
            request.metadata["execution_profile"] = json!(crate::strict::PROFILE);
        }
        if let Some(value) = startup {
            request.metadata["presentation_startup"] = value;
        }
        let mut report = report();
        let mut session = None;
        let playback = Playback::new(&installation, &request, &mut report, &mut session).unwrap();
        match (Desktop::new(playback), expected) {
            (Ok(driver), Ok(clock)) => assert_eq!(driver.clock(), clock),
            (Err(error), Err(fragment)) => assert!(error.contains(fragment), "{error}"),
            (Ok(driver), Err(_)) => panic!("accepted {:?}", driver.clock()),
            (Err(error), Ok(_)) => panic!("rejected: {error}"),
        }
    }
}

fn state(driver: &mut Desktop<'_>) -> serde_json::Value {
    let session = driver.playback.active().unwrap();
    json!({"world":session.observe("world.snapshot").unwrap(),
        "managed":session.observe("managed.snapshot").unwrap(),
        "input":session.request(json!({"op":"input.get"})).unwrap()})
}

fn tap(sequence: u64, pressed: bool) -> LiveInputSample {
    LiveInputSample {
        sequence,
        timestamp: Duration::ZERO,
        origin: IngressOrigin::OsEventLoop,
        event: LiveInputEvent::Key {
            key: "w".into(),
            pressed,
            repeat: false,
        },
    }
}

#[test]
fn actual_initial_scene_extraction_never_advances_captures_or_submits_input() {
    let installation = crate::test_support::installation();
    let artifacts = crate::test_support::scratch("desktop-");
    let mut request = request(artifacts.path());
    request.metadata["execution_profile"] = json!(crate::strict::PROFILE);
    request.metadata["presentation_startup"] = json!("ready_prewarmed_v1");
    request.seed = 17;
    let mut report = report();
    let mut session = None;
    let playback = Playback::new(&installation, &request, &mut report, &mut session).unwrap();
    let mut driver = Desktop::new(playback).unwrap();
    let viewport = tick(0, vec![]).viewport;
    driver.initialize(viewport).unwrap();
    let before = state(&mut driver);
    for _ in 0..2 {
        let frame = driver.prepare_initial(viewport).unwrap().unwrap();
        assert!(frame.capture.is_none() && !frame.exit && frame.textures.is_some());
        assert!(!frame.quads.is_empty());
        assert_eq!(
            state(&mut driver),
            before,
            "extraction must not mutate native state"
        );
    }
    assert!(driver.playback.pending_capture.is_none());
    driver.ready().unwrap();
    driver.ingress(&tap(1, true)).unwrap();
    driver.ingress(&tap(2, false)).unwrap();
    let mut first = tick(0, vec![]);
    first.dt = Duration::from_nanos(1);
    let output = driver.update(&first).unwrap();
    let input = driver
        .playback
        .active()
        .unwrap()
        .request(json!({"op":"input.get"}))
        .unwrap();
    assert_eq!(input["down"], json!(["W"]));
    assert_eq!(input["last_event_id"], 2);
    assert_ne!(
        state(&mut driver)["world"],
        before["world"],
        "first update advanced"
    );
    present(&mut driver, output, 0);
    assert_startup_order(artifacts.path());
    assert_cleanup(&mut driver);
}

fn assert_startup_order(artifacts: &Path) {
    let journal = std::fs::read_to_string(artifacts.join("interactive-events.ndjson")).unwrap();
    let stages: Vec<String> = journal
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["stage"].to_string())
        .collect();
    let position = |stage: &str| stages.iter().position(|s| s == stage).unwrap();
    let prepared = position("\"startup_prepared\"");
    let ready = position("\"ready\"");
    let update = position("\"update\"");
    assert!(prepared < ready && ready < update, "{stages:?}");
}

fn assert_cleanup(driver: &mut Desktop<'_>) {
    driver.playback.active().unwrap().unload().unwrap();
    assert_eq!(driver.playback.active().unwrap().statistics().unwrap().0, 0);
    assert_eq!(
        driver.playback.active().unwrap().managed_roots().unwrap(),
        0
    );
}
