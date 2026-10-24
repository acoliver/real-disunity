use super::{
    Desktop,
    tests::{present, report, request, tick},
};
use crate::playback::Playback;
use serde_json::json;
use std::{path::Path, time::Duration};
use worldrunner::scene::viewer::{IngressOrigin, LiveDriver, LiveInputEvent, LiveInputSample};

fn sample(sequence: u64, pressed: bool) -> LiveInputSample {
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
fn actual_strict_desktop_rejects_stall_without_mutation_or_losing_a_staged_tap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let artifacts = tempfile::tempdir_in(root.join(".work/tmp")).unwrap();
    let mut request = request(&root, artifacts.path());
    request.metadata["execution_profile"] = json!(crate::strict::PROFILE);
    request.seed = 17;
    let mut report = report();
    let mut session = None;
    let playback = Playback::new(&root, &request, &mut report, &mut session).unwrap();
    let mut driver = Desktop::new(playback).unwrap();
    driver.initialize(tick(0, vec![]).viewport).unwrap();
    driver.ingress(&sample(1, true)).unwrap();
    driver.ingress(&sample(2, false)).unwrap();
    for nanos in [249_999_991, 4_000_000_000] {
        reject_elapsed(&mut driver, nanos);
    }
    let journal = artifacts.path().join("interactive-events.ndjson");
    std::fs::remove_file(&journal).unwrap();
    std::fs::create_dir(&journal).unwrap();
    reject_elapsed_write_failure(&mut driver);
    std::fs::remove_dir(&journal).unwrap();
    let profile = worldrunner::scene::viewer::cpu_profile::CpuProfile::begin().unwrap();
    let mut first = tick(0, vec![]);
    first.dt = Duration::from_nanos(1);
    let output = driver.update(&first).unwrap();
    let input = driver
        .playback
        .active()
        .unwrap()
        .request(json!({"op":"input.get"}))
        .unwrap();
    assert_eq!(input["keys"], json!([]));
    assert_eq!(input["down"], json!(["W"]));
    assert_eq!(input["up"], json!(["W"]));
    assert_eq!(input["last_event_id"], 2);
    let food = random_food(&mut driver);
    present(&mut driver, output, 0);
    check_spans(profile);
    reject_malformed_batch(&mut driver);
    check_random_control(&mut driver, &food);
    driver.playback.active().unwrap().unload().unwrap();
    assert_eq!(driver.playback.active().unwrap().statistics().unwrap().0, 0);
    assert_eq!(
        driver.playback.active().unwrap().managed_roots().unwrap(),
        0
    );
}

fn reject_malformed_batch(driver: &mut Desktop<'_>) {
    let before = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    let mut next = sample(3, true);
    next.timestamp = Duration::from_millis(1);
    driver.ingress(&next).unwrap();
    next.sequence = 2;
    driver.ingress(&next).unwrap();
    assert!(
        driver
            .update(&tick(1, vec![]))
            .err()
            .unwrap()
            .contains("backwards")
    );
    assert_eq!(
        before,
        driver
            .playback
            .active()
            .unwrap()
            .observe("world.snapshot")
            .unwrap()
    );
}

#[test]
fn common_acquisition_clock_is_explicitly_selected_only_for_strict_desktop() {
    use worldrunner::scene::viewer::LiveClock;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let artifacts = tempfile::tempdir_in(root.join(".work/tmp")).unwrap();
    for strict in [false, true] {
        let mut request = request(&root, artifacts.path());
        if strict {
            request.metadata["execution_profile"] = json!(crate::strict::PROFILE);
        }
        let mut report = report();
        let mut session = None;
        let playback = Playback::new(&root, &request, &mut report, &mut session).unwrap();
        let driver = Desktop::new(playback).unwrap();
        let expected = if strict {
            LiveClock::Acquisition
        } else {
            LiveClock::Legacy
        };
        assert_eq!(driver.clock(), expected);
        assert!(driver.playback.session.is_none());
    }
}

fn native_state(driver: &mut Desktop<'_>) -> serde_json::Value {
    let session = driver.playback.active().unwrap();
    json!({"world":session.observe("world.snapshot").unwrap(),
        "managed":session.observe("managed.snapshot").unwrap(),
        "input":session.request(json!({"op":"input.get"})).unwrap()})
}

fn reject_elapsed(driver: &mut Desktop<'_>, nanos: u64) {
    let before = native_state(driver);
    let mut stalled = tick(1, vec![LiveInputEvent::CaptureRequested(99)]);
    stalled.dt = Duration::from_nanos(nanos);
    let error = driver.update(&stalled).err().unwrap();
    assert_eq!(error, "strict catch-up exceeds 15 exact 16666666ns chunks");
    assert_eq!(before, native_state(driver));
    assert_eq!(driver.cursor, 1);
    assert!(driver.playback.pending_capture.is_none());
    assert!(driver.playback.report.captures.is_empty());
    let path = driver
        .playback
        .request
        .artifact_root
        .join("interactive-events.ndjson");
    let text = std::fs::read_to_string(path).expect("rejection must be journaled");
    let row: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(row["stage"], "strict_elapsed_rejected");
    assert_eq!(row["dt_ns"], nanos.to_string());
    assert_eq!(row["elapsed_ns"], "20000000");
    assert_eq!(row["endpoint_ns"], (nanos + 20_000_000).to_string());
    assert_eq!(row["cap_ns"], "249999990");
    assert_eq!(
        row["required_chunks"],
        ((nanos - 1) / 16_666_666 + 1).to_string()
    );
    assert_eq!(row["frame"], 1);
    assert_eq!(row["error"], error);
}

fn reject_elapsed_write_failure(driver: &mut Desktop<'_>) {
    let before = native_state(driver);
    let mut stalled = tick(1, vec![LiveInputEvent::CaptureRequested(99)]);
    stalled.dt = Duration::from_secs(4);
    assert_eq!(
        driver.update(&stalled).err().unwrap(),
        "strict catch-up exceeds 15 exact 16666666ns chunks"
    );
    assert_eq!(before, native_state(driver));
    assert!(driver.playback.report.diagnostics.iter().any(|error| {
        error
            .message
            .contains("strict_elapsed_rejected diagnostic write failed")
    }));
}

fn random_food(driver: &mut Desktop<'_>) -> serde_json::Value {
    let session = driver.playback.active().unwrap();
    json!([
        session
            .observe("managed:FoodSpawner:CurrentFoodPos.x")
            .unwrap(),
        session
            .observe("managed:FoodSpawner:CurrentFoodPos.y")
            .unwrap()
    ])
}

fn check_random_control(driver: &mut Desktop<'_>, food: &serde_json::Value) {
    driver.playback.active().unwrap().reload().unwrap();
    driver
        .playback
        .active()
        .unwrap()
        .advance_rational(1, 1_000_000_000)
        .unwrap();
    assert_eq!(
        &random_food(driver),
        food,
        "rejected stalls preserve the seeded native random outcome"
    );
}

fn check_spans(profile: worldrunner::scene::viewer::cpu_profile::CpuProfile) {
    let (rows, dropped) = profile.finish();
    assert_eq!(dropped, 0);
    for stage in [
        "native_advance",
        "driver_update",
        "contribution",
        "asset_load",
        "snapshot",
        "ui_layout",
        "sprite_extract",
        "glyph_extract",
        "png_write",
        "driver_presented",
    ] {
        let row = rows.iter().find(|row| row.stage == stage).expect(stage);
        assert_eq!(row.frame, 0);
        assert!(row.duration.as_nanos().to_string().parse::<u128>().is_ok());
    }
}

#[test]
fn elapsed_rejection_precedes_pending_timeline_load_and_invalid_input_batch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let artifacts = tempfile::tempdir_in(root.join(".work/tmp")).unwrap();
    let mut request = request(&root, artifacts.path());
    request
        .metadata
        .as_object_mut()
        .unwrap()
        .remove("interactive");
    request.metadata["execution_profile"] = json!(crate::strict::PROFILE);
    let mut report = report();
    let mut session = None;
    let playback = Playback::new(&root, &request, &mut report, &mut session).unwrap();
    let mut driver = Desktop::new(playback).unwrap();
    driver.ingress(&sample(2, true)).unwrap();
    driver.ingress(&sample(1, false)).unwrap();
    let mut stalled = tick(0, vec![]);
    stalled.dt = Duration::from_nanos(249_999_991);
    assert_eq!(
        driver.update(&stalled).err().unwrap(),
        "strict catch-up exceeds 15 exact 16666666ns chunks"
    );
    assert_eq!(driver.cursor, 0);
    assert!(driver.playback.session.is_none());
    assert!(driver.playback.pending_capture.is_none());
    assert!(driver.playback.report.observations.is_empty());
    assert!(driver.playback.report.captures.is_empty());
}
