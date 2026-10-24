use std::{sync::mpsc, time::Duration};
use worldrunner_scene_viewer::{LiveInputEvent, LiveOptions, ScheduledInput, run_live};

#[path = "driver.rs"]
mod driver;
#[path = "evidence.rs"]
mod evidence;
#[path = "replay.rs"]
mod replay;

pub(super) fn run() {
    let mut options = LiveOptions::new(evidence::TITLE, [128, 64]);
    options.fixed_dt = Some(Duration::from_millis(20));
    options.max_frames = Some(8);
    // Deliberately unsorted. Admission preserves stable event order per frame.
    options.scheduled_input = vec![key(3, false), key(1, true)];
    let (sender, receiver) = mpsc::channel();
    options.input = Some(receiver);
    let mut driver = driver::Driver::new(sender);
    run_live(&mut driver, options).expect("actual desktop live host");
    assert_eq!(driver.initialized, 1);
    assert_eq!(driver.ticks.len(), 8);
    assert_eq!(driver.receipts, (0..8).collect::<Vec<_>>());
    assert!(
        driver.os_pressed && driver.os_released,
        "real OS arrow press/release reaches callback"
    );
    assert!(!driver.held.contains("a"));
    assert!(
        driver
            .ticks
            .iter()
            .any(|tick| tick.input.contains(&LiveInputEvent::CaptureRequested(909)))
    );
    assert!(
        driver
            .ticks
            .iter()
            .any(|tick| tick.input.contains(&LiveInputEvent::Resized([192, 96])))
    );
    assert_eq!(driver.ticks.last().unwrap().viewport.width, 192);
    assert_eq!(driver.ticks.last().unwrap().viewport.height, 96);
    replay::verify(&driver);
    println!(
        "live window: 8 updates, 8 same-frame GPU captures, retained texture, scheduled A, OS Right press/release, native resize, channel capture and pixel-identical headless replay passed"
    );
}

fn key(frame: u64, pressed: bool) -> ScheduledInput {
    ScheduledInput {
        frame,
        event: LiveInputEvent::Key {
            key: "a".into(),
            pressed,
            repeat: false,
        },
    }
}
