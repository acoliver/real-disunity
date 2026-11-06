use super::*;

#[derive(Default)]
struct FakeClock(Duration);

impl FakeClock {
    fn advance(&mut self, seconds: u64) -> Duration {
        self.0 += Duration::from_secs(seconds);
        self.0
    }
}

#[test]
fn slow_observable_progress_passes_beyond_sixty_seconds() {
    let mut clock = FakeClock::default();
    let mut progress = Progress::default();
    let mut watchdog = ProgressWatchdog::new("capture 7 generation 2".into(), progress.clone());
    for step in 1..=5 {
        progress.submitted_frame = step;
        assert!(watchdog.check(clock.advance(120), progress.clone()).is_ok());
    }
    progress.capture_delivered = true;
    assert!(watchdog.check(clock.advance(120), progress).is_ok());
    assert_eq!(clock.0, Duration::from_mins(12));
}

#[test]
fn no_progress_fails_at_stall_window_and_names_the_signal() {
    let mut clock = FakeClock::default();
    let progress = Progress::default();
    let mut watchdog = ProgressWatchdog::new("capture 7 generation 2".into(), progress.clone());
    assert!(watchdog.check(clock.advance(179), progress.clone()).is_ok());
    let failure = watchdog.check(clock.advance(1), progress).unwrap_err();
    assert!(failure.contains("capture 7 generation 2"), "{failure}");
    assert!(failure.contains("no observable progress"), "{failure}");
    assert!(failure.contains("stall 180s"), "{failure}");
    assert!(failure.contains("total 180s"), "{failure}");
}

#[test]
fn endless_progress_hits_total_bound_even_when_capture_arrives() {
    for delivered in [false, true] {
        let mut clock = FakeClock::default();
        let mut progress = Progress::default();
        let mut watchdog = ProgressWatchdog::new("recipe capture 9".into(), progress.clone());
        for step in 1..12 {
            progress.submitted_frame = step;
            assert!(watchdog.check(clock.advance(100), progress.clone()).is_ok());
        }
        progress.submitted_frame += 1;
        progress.capture_delivered = delivered;
        let failure = watchdog.check(clock.advance(100), progress).unwrap_err();
        assert!(failure.contains("recipe capture 9"), "{failure}");
        assert!(failure.contains("total bound reached"), "{failure}");
        assert!(failure.contains("stall 100s"), "{failure}");
        assert!(failure.contains("total 1200s"), "{failure}");
    }
}

#[test]
fn each_observable_signal_resets_stall_but_unchanged_polls_do_not() {
    let signals = [
        Progress {
            submitted_frame: 1,
            ..Progress::default()
        },
        Progress {
            accepted_generation: 1,
            ..Progress::default()
        },
        Progress {
            ready_generation: 1,
            ..Progress::default()
        },
        Progress {
            preparations_in_flight: 1,
            ..Progress::default()
        },
        Progress {
            capture_delivered: true,
            ..Progress::default()
        },
    ];
    for progress in signals {
        let mut clock = FakeClock::default();
        let mut watchdog =
            ProgressWatchdog::new("capture progress signal".into(), Progress::default());
        assert!(watchdog.check(clock.advance(179), progress.clone()).is_ok());
        assert!(watchdog.check(clock.advance(179), progress.clone()).is_ok());
        let failure = watchdog.check(clock.advance(1), progress).unwrap_err();
        assert!(failure.contains("stall 180s"), "{failure}");
        assert!(failure.contains("total 359s"), "{failure}");
    }
}
