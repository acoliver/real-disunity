use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use worldrunner_script::{
    GameScripts, MonotonicInput, SchedulerNames, SchedulerPolicy, ScriptConfig,
};

fn scripted_input(
    samples: impl IntoIterator<Item = io::Result<Duration>>,
) -> (MonotonicInput, Arc<AtomicUsize>) {
    let samples = Arc::new(Mutex::new(samples.into_iter().collect::<VecDeque<_>>()));
    let reads = Arc::new(AtomicUsize::new(0));
    let input_reads = Arc::clone(&reads);
    let input: MonotonicInput = Arc::new(move || {
        input_reads.fetch_add(1, Ordering::SeqCst);
        samples
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted sample")
    });
    (input, reads)
}

fn millis(value: u64) -> io::Result<Duration> {
    Ok(Duration::from_millis(value))
}

fn mutable_input(value: Duration) -> (MonotonicInput, Arc<Mutex<Duration>>, Arc<AtomicUsize>) {
    let sample = Arc::new(Mutex::new(value));
    let reads = Arc::new(AtomicUsize::new(0));
    let captured_sample = Arc::clone(&sample);
    let captured_reads = Arc::clone(&reads);
    let input: MonotonicInput = Arc::new(move || {
        captured_reads.fetch_add(1, Ordering::SeqCst);
        Ok(*captured_sample.lock().unwrap())
    });
    (input, sample, reads)
}

fn boot(input: MonotonicInput) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_monotonic_millis("GetClock", input)
        .unwrap();
    scripts.add_source("source", &["GetClock"]).unwrap();
    scripts
        .run(
            "source",
            "read.lua",
            b"function Read() return GetClock() end",
        )
        .unwrap();
    scripts
}

fn read(scripts: &mut GameScripts) -> i64 {
    scripts.call("Read", ()).unwrap()
}

#[test]
fn samples_once_for_registration_and_once_for_a_valid_zero_argument_call() {
    let (input, reads) =
        scripted_input([millis(9_000), millis(9_000), millis(9_000), millis(9_000)]);
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_monotonic_millis("GetClock", input)
        .unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    scripts.add_source("source", &["GetClock"]).unwrap();
    scripts
        .run(
            "source",
            "monotonic.lua",
            b"Checks = { select('#', GetClock()), math.type(GetClock()), GetClock() }",
        )
        .unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 4);
    assert_eq!(
        scripts.eval_json("source", "return Checks").unwrap(),
        serde_json::json!([1, "integer", 9000])
    );
}

#[test]
fn validation_sample_is_discarded_and_calls_are_ready_before_advancement() {
    let (input, reads) = scripted_input([millis(9_000), millis(9_037), millis(9_037)]);
    let mut scripts = boot(input);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(scripts.now(), 0);
    assert_eq!(read(&mut scripts), 9_037);
    assert_eq!(read(&mut scripts), 9_037);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(scripts.now(), 0);
}

#[test]
fn extra_arguments_including_nil_are_rejected_without_a_read() {
    let (input, reads) = scripted_input([millis(1), millis(2)]);
    let mut scripts = boot(input);
    for args in ["nil", "1", "nil, nil", "{}", "false", "'extra'"] {
        let error = scripts
            .eval_json("source", &format!("return GetClock({args})"))
            .unwrap_err();
        assert!(
            error.to_string().contains("expected zero arguments"),
            "{error}"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
    assert_eq!(read(&mut scripts), 2);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[test]
fn registration_rejects_duplicate_nested_invalid_and_late_names_before_sampling() {
    let (input, _, reads) = mutable_input(Duration::from_millis(42));
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_monotonic_millis("clock.Read", Arc::clone(&input))
        .unwrap();
    for (name, message) in [
        ("clock.Read", "registered twice"),
        ("clock", "nests with native"),
        ("clock.Read.Child", "nests with native"),
        ("bad name", "not a dotted Lua name"),
        ("", "not a dotted Lua name"),
    ] {
        let error = scripts
            .register_monotonic_millis(name, Arc::clone(&input))
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
    assert!(
        scripts
            .register_native("clock.Read", |_, ()| Ok(()))
            .is_err()
    );
    scripts.register_native("Other", |_, ()| Ok(())).unwrap();
    assert!(
        scripts
            .register_monotonic_millis("Other", Arc::clone(&input))
            .is_err()
    );
    scripts.add_source("source", &["clock.Read"]).unwrap();
    let error = scripts
        .register_monotonic_millis("Late", input)
        .unwrap_err();
    assert!(error.to_string().contains("after sources"), "{error}");
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        scripts.eval_json("source", "return clock.Read()").unwrap(),
        42
    );
}

#[test]
fn integer_millisecond_boundaries_do_not_wrap_or_lose_precision() {
    let maximum = u64::try_from(i64::MAX).unwrap();
    let (input, reads) = scripted_input([
        millis(maximum),
        millis(maximum),
        Ok(Duration::from_millis(maximum) + Duration::from_nanos(999_999)),
        millis(maximum + 1),
        Ok(Duration::MAX),
        millis(7),
    ]);
    let mut scripts = boot(input);
    assert_eq!(read(&mut scripts), i64::MAX);
    assert_eq!(read(&mut scripts), i64::MAX);
    for _ in 0..2 {
        let error = scripts.call::<i64>("Read", ()).unwrap_err();
        assert!(
            error.to_string().contains("exceeds i64::MAX milliseconds"),
            "{error}"
        );
    }
    assert_eq!(read(&mut scripts), 7);
    assert_eq!(reads.load(Ordering::SeqCst), 6);
}

#[test]
fn invalid_initial_samples_do_not_install_a_native() {
    for sample in [
        millis(u64::try_from(i64::MAX).unwrap() + 1),
        Ok(Duration::MAX),
    ] {
        let (input, reads) = scripted_input([sample]);
        let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
        let error = scripts
            .register_monotonic_millis("GetClock", input)
            .unwrap_err();
        assert!(
            error.to_string().contains("validating monotonic input"),
            "{error}"
        );
        assert!(error.to_string().contains("exceeds i64::MAX"), "{error}");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        scripts.register_native("GetClock", |_, ()| Ok(3)).unwrap();
    }
}

#[test]
fn fractional_samples_floor_and_preserve_equal_zero_and_nonzero_origins() {
    let (input, sample, reads) = mutable_input(Duration::ZERO);
    let mut scripts = boot(input);
    for (nanos, expected) in [
        (0, 0),
        (999_999, 0),
        (1_000_000, 1),
        (1_000_000, 1),
        (9_000_999_999, 9_000),
    ] {
        *sample.lock().unwrap() = Duration::from_nanos(nanos);
        assert_eq!(read(&mut scripts), expected);
    }
    assert_eq!(reads.load(Ordering::SeqCst), 6);
    let opening = read(&mut scripts);
    *sample.lock().unwrap() = Duration::from_millis(9_037);
    assert_eq!(read(&mut scripts) - opening, 37);
    assert_eq!(scripts.now(), 0);
}

#[test]
fn initial_and_later_input_errors_are_visible_without_fallback() {
    for kind in [
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::Unsupported,
        io::ErrorKind::InvalidData,
    ] {
        let (input, reads) = scripted_input([Err(io::Error::new(kind, "initial source failure"))]);
        let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
        let error = scripts
            .register_monotonic_millis("GetClock", input)
            .unwrap_err();
        assert!(
            error.to_string().contains("initial source failure"),
            "{error}"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(scripts.add_source("source", &["GetClock"]).is_err());
    }
    let (input, reads) = scripted_input([
        millis(9),
        millis(10),
        Err(io::Error::other("later source failure")),
        Err(io::Error::other("caught source failure")),
        millis(12),
    ]);
    let mut scripts = boot(input);
    assert_eq!(read(&mut scripts), 10);
    let error = scripts.call::<i64>("Read", ()).unwrap_err();
    assert!(
        error.to_string().contains("reading monotonic input"),
        "{error}"
    );
    assert!(
        error.to_string().contains("later source failure"),
        "{error}"
    );
    assert_eq!(scripts.eval_json("source", "local ok, err = pcall(GetClock) return not ok and tostring(err):find('caught source failure') ~= nil").unwrap(), true);
    assert_eq!(read(&mut scripts), 12);
    assert_eq!(reads.load(Ordering::SeqCst), 5);
}

fn scheduled(input: MonotonicInput) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_monotonic_millis("GetClock", input)
        .unwrap();
    scripts
        .register_scheduler(
            &SchedulerNames {
                spawn: "Spawn".into(),
                sleep: "Sleep".into(),
                wakeup: "Wake".into(),
                kill: "Kill".into(),
                current: "Current".into(),
                now: "Now".into(),
            },
            &SchedulerPolicy::default(),
        )
        .unwrap();
    scripts
        .add_source("source", &["GetClock", "Spawn", "Sleep", "Now"])
        .unwrap();
    scripts
        .run(
            "source",
            "read.lua",
            b"function Read() return GetClock() end",
        )
        .unwrap();
    scripts
}

#[test]
fn stored_zero_consumer_stays_parked_after_later_positive_input() {
    let (input, sample, reads) = mutable_input(Duration::ZERO);
    let mut scripts = scheduled(input);
    scripts
        .run(
            "source",
            "consumer.lua",
            br#"
        Completed = false
        Opening = GetClock()
        Spawn(function()
            AtStart = {Now(), GetClock()}
            while Opening == 0 do Sleep(0) end
            Completed = true
        end)
    "#,
        )
        .unwrap();
    assert!(scripts.advance(1).unwrap().is_empty());
    assert_eq!(
        scripts.eval_json("source", "return AtStart").unwrap(),
        serde_json::json!([0, 0])
    );
    *sample.lock().unwrap() = Duration::from_millis(100);
    for _ in 0..3 {
        assert!(scripts.advance(1).unwrap().is_empty());
    }
    assert_eq!(read(&mut scripts), 100);
    assert_eq!(
        scripts
            .eval_json("source", "return {Opening, Completed}")
            .unwrap(),
        serde_json::json!([0, false])
    );
    assert_eq!(scripts.thread_count(), 1);
    assert_eq!(reads.load(Ordering::SeqCst), 4);
}

#[test]
fn source_progress_is_independent_of_pause_advance_reset_and_overflow() {
    let (input, sample, reads) = mutable_input(Duration::from_millis(9_000));
    let mut scripts = scheduled(input);
    scripts.set_clock_paused("game", true).unwrap();
    let opening = read(&mut scripts);
    *sample.lock().unwrap() = Duration::from_millis(9_037);
    assert_eq!(read(&mut scripts) - opening, 37);
    assert!(scripts.advance(500).unwrap().is_empty());
    assert_eq!(scripts.now(), 0);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    scripts.set_clock_paused("game", false).unwrap();
    assert!(scripts.advance(600).unwrap().is_empty());
    assert_eq!(scripts.now(), 600);
    assert_eq!(read(&mut scripts), 9_037);
    scripts.set_clock("game", 0).unwrap();
    assert_eq!(read(&mut scripts), 9_037);
    scripts.set_clock("game", u64::MAX).unwrap();
    assert!(scripts.advance(1).is_err());
    assert_eq!(read(&mut scripts), 9_037);
    assert_eq!(*sample.lock().unwrap(), Duration::from_millis(9_037));
}

#[test]
fn independent_boots_do_not_share_inputs_or_schedulers() {
    let (a, value_a, reads_a) = mutable_input(Duration::from_millis(20));
    let (b, value_b, reads_b) = mutable_input(Duration::from_millis(80));
    assert!(!Arc::ptr_eq(&a, &b));
    let mut first = boot(a);
    let mut second = boot(b);
    *value_a.lock().unwrap() = Duration::from_millis(30);
    assert_eq!(read(&mut first), 30);
    assert_eq!(read(&mut second), 80);
    assert_eq!(*value_b.lock().unwrap(), Duration::from_millis(80));
    first.advance(5).unwrap();
    assert_eq!(first.now(), 5);
    assert_eq!(second.now(), 0);
    assert_eq!(reads_a.load(Ordering::SeqCst), 2);
    assert_eq!(reads_b.load(Ordering::SeqCst), 2);
}

#[test]
fn shared_input_keeps_identity_origin_and_lifetime_across_boots() {
    let (input, sample, reads) = mutable_input(Duration::from_millis(9_000));
    let weak = Arc::downgrade(&input);
    let other = Arc::clone(&input);
    assert!(Arc::ptr_eq(&input, &other));
    let mut first = boot(input);
    *sample.lock().unwrap() = Duration::from_millis(9_037);
    let mut second = boot(other);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert_eq!(read(&mut first), 9_037);
    assert_eq!(read(&mut second), 9_037);
    drop(first);
    assert!(weak.upgrade().is_some());
    *sample.lock().unwrap() = Duration::from_millis(9_100);
    assert_eq!(read(&mut second), 9_100);
    drop(second);
    assert!(weak.upgrade().is_none());
}

#[test]
fn source_grants_share_the_registered_function_and_ungranted_sources_cannot_read() {
    let (input, _, reads) = mutable_input(Duration::from_millis(50));
    let mut scripts = boot(input);
    scripts.add_source("another", &["GetClock"]).unwrap();
    scripts.add_source("denied", &[]).unwrap();
    scripts
        .run("source", "identity.lua", b"SavedClock = GetClock")
        .unwrap();
    assert_eq!(
        scripts
            .eval_json("another", "return SavedClock == GetClock")
            .unwrap(),
        true
    );
    assert_eq!(
        scripts
            .eval_json("denied", "return GetClock == nil")
            .unwrap(),
        true
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn source_errors_in_scheduled_work_are_reported_as_thread_failures() {
    let (input, reads) = scripted_input([
        millis(20),
        Err(io::Error::other("scheduled source failure")),
    ]);
    let mut scripts = scheduled(input);
    scripts
        .run(
            "source",
            "scheduled.lua",
            b"Spawn(function() GetClock() end)",
        )
        .unwrap();
    let failures = scripts.advance(1).unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message.contains("scheduled source failure"));
    assert_eq!(scripts.thread_count(), 0);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}
