use super::{Driver, IngressOrigin, LiveInputEvent, LiveInputSample};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn window(title: &str) -> Option<String> {
    let output = Command::new("xdotool")
        .args(["search", "--name", title])
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .last()
        .map(str::to_owned)
}

/// Blocks until the X server reports a mapped window named `title`, the condition focusing it
/// needs. `xdotool search --sync --onlyvisible` waits on the server itself, so a slow start on
/// a loaded runner only delays the test. The sink exiting first fails at once with its status,
/// and the guard only stops a run that would otherwise hang.
pub(super) fn wait_window(title: &str, sink: &mut Child) {
    const HANG_GUARD: Duration = Duration::from_mins(2);
    let mut search = Command::new("xdotool")
        .args(["search", "--sync", "--onlyvisible", "--name", title])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let outcome = loop {
        if let Some(status) = search.try_wait().unwrap() {
            break status
                .success()
                .then_some(())
                .ok_or(format!("xdotool search failed: {status}"));
        }
        if let Some(status) = sink.try_wait().unwrap() {
            break Err(format!(
                "the focus sink exited before mapping its window: {status}"
            ));
        }
        if started.elapsed() > HANG_GUARD {
            break Err(format!("no mapped focus sink window after {HANG_GUARD:?}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if outcome.is_err() {
        let _ = search.kill();
    }
    let _ = search.wait();
    if let Err(message) = outcome {
        panic!("{message}");
    }
}

pub(super) fn xdo(args: &[&str]) {
    assert!(
        Command::new("xdotool")
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

pub(super) fn focus(title: &str) {
    xdo(&["windowfocus", "--sync", &window(title).unwrap()]);
}

fn keys<'a>(batch: &'a [LiveInputSample], name: &'a str) -> Vec<(bool, bool)> {
    batch
        .iter()
        .filter_map(|sample| match &sample.event {
            LiveInputEvent::Key {
                key,
                pressed,
                repeat,
            } if key == name => Some((*pressed, *repeat)),
            _ => None,
        })
        .collect()
}

pub(super) fn verify(driver: &Driver) {
    let all = driver.batches.iter().flatten().collect::<Vec<_>>();
    for (index, sample) in all.iter().enumerate() {
        assert_eq!(sample.sequence, u64::try_from(index).unwrap() + 1);
        assert_ne!(sample.origin, IngressOrigin::ScheduledReplay);
    }
    assert!(all.windows(2).all(|p| p[0].timestamp <= p[1].timestamp));
    verify_transitions(driver);
    verify_provenance(&all);
    if driver.clock == super::LiveClock::FrameDelta {
        assert!(driver.ticks[0].dt.is_zero());
    } else {
        assert!(!driver.ticks[0].dt.is_zero());
        for (tick, batch) in driver.ticks.iter().zip(&driver.batches) {
            let endpoint = tick.elapsed + tick.dt;
            assert!(
                batch
                    .iter()
                    .filter(|s| s.origin == IngressOrigin::OsEventLoop)
                    .all(|s| s.timestamp <= endpoint)
            );
        }
    }
    for pair in driver.ticks.windows(2) {
        assert_eq!(pair[1].elapsed, pair[0].elapsed + pair[0].dt);
    }
}

fn verify_transitions(driver: &Driver) {
    assert_eq!(
        keys(&driver.batches[1], "a"),
        [(true, false), (false, false)]
    );
    let held = driver
        .batches
        .iter()
        .flat_map(|batch| keys(batch, "b"))
        .collect::<Vec<_>>();
    assert_eq!(held.first(), Some(&(true, false)));
    assert_eq!(held.last(), Some(&(false, false)));
    assert!(
        driver.batches[5]
            .iter()
            .any(|s| s.event == LiveInputEvent::Focused(false))
    );
    assert!(
        driver.batches[6]
            .iter()
            .any(|s| s.event == LiveInputEvent::Focused(true))
    );
    for name in ["f5", "f12", "escape"] {
        assert_eq!(
            keys(&driver.batches[7], name),
            [(true, false), (false, false)]
        );
    }
    assert_eq!(keys(&driver.batches[8], "c"), [(true, true)]);
    verify_focus_while_held(&driver.batches[5]);
}

fn verify_provenance(all: &[&LiveInputSample]) {
    let capture = all
        .iter()
        .find(|s| s.event == LiveInputEvent::CaptureRequested(91))
        .unwrap();
    assert_eq!(capture.origin, IngressOrigin::ChannelDelivery);
    assert!(
        all.iter()
            .filter(|s| matches!(&s.event,
                LiveInputEvent::Key { key, .. } if key != "c")
                || matches!(s.event, LiveInputEvent::Focused(_)))
            .all(|s| s.origin == IngressOrigin::OsEventLoop)
    );
}

fn verify_focus_while_held(batch: &[LiveInputSample]) {
    let down = batch
        .iter()
        .position(|s| {
            matches!(&s.event,
        LiveInputEvent::Key { key, pressed: true, .. } if key == "d")
        })
        .unwrap();
    let lost = batch
        .iter()
        .position(|s| s.event == LiveInputEvent::Focused(false))
        .unwrap();
    assert!(down < lost, "acquired key press precedes focus loss");
}
