//! What a script decides for each update of a window, without a game or a viewer: which updates
//! advance the game, which keys they deliver, how long a step they take, and when the run is
//! finished. A capture holds the game still until it arrives, so the frame it shows is the frame
//! it was asked for.

use super::*;
use std::collections::BTreeSet;
use worldrunner_scene_viewer::RetainedCaptureOutcome;

const KEY: u16 = 119;

fn window(max_frames: u64, captures: &[u64], keys: &[ScheduledKey]) -> PlayerWindow {
    PlayerWindow {
        title: "script".into(),
        size: [8, 4],
        fixed_dt: Some(Duration::from_millis(10)),
        max_frames: Some(max_frames),
        capture_frames: captures.iter().copied().collect::<BTreeSet<_>>(),
        keys: keys.to_vec(),
    }
}

/// The update numbered `frame` by the viewer, which took `dt` since the one before.
fn tick(frame: u64, dt_ms: u64) -> RetainedTick {
    RetainedTick {
        frame,
        elapsed: Duration::from_millis(dt_ms * frame),
        dt: Duration::from_millis(dt_ms),
    }
}

/// The game frame an update advanced to, or `None` for an update that held the game.
fn frame_of(turn: &Turn) -> Option<u64> {
    match turn {
        Turn::Advance { frame, .. } => Some(*frame),
        Turn::Held => None,
    }
}

#[test]
fn updates_after_a_capture_request_leave_the_game_alone_until_the_capture_arrives() {
    let mut script = WindowScript::new(&window(6, &[1], &[]));

    let first = [script.next(&tick(0, 10)), script.next(&tick(1, 10))];
    assert_eq!(first.each_ref().map(frame_of), [Some(0), Some(1)]);
    assert_eq!(script.due_capture(), Some(1));
    script.requested(7, 1);
    let held = [script.next(&tick(2, 10)), script.next(&tick(3, 10))];

    assert_eq!(held, [Turn::Held, Turn::Held]);
    assert_eq!(script.advanced(), 2, "the held updates are not game frames");
    script.delivered(7, &RetainedCaptureOutcome::Cancelled);
    assert_eq!(frame_of(&script.next(&tick(4, 10))), Some(2));
}

#[test]
fn a_scheduled_key_arrives_at_its_game_frame_however_many_updates_were_held() {
    let key = ScheduledKey {
        frame: 2,
        code: KEY,
        pressed: true,
    };
    let mut script = WindowScript::new(&window(5, &[0], &[key]));

    assert_eq!(frame_of(&script.next(&tick(0, 10))), Some(0));
    script.requested(1, 0);
    for update in 1..=3 {
        assert_eq!(
            script.next(&tick(update, 10)),
            Turn::Held,
            "update {update}"
        );
    }
    script.delivered(1, &RetainedCaptureOutcome::Cancelled);
    let second = script.next(&tick(4, 10));
    let third = script.next(&tick(5, 10));

    assert!(matches!(&second, Turn::Advance { frame: 1, keys, .. } if keys.is_empty()));
    assert_eq!(
        third,
        Turn::Advance {
            frame: 2,
            keys: vec![(KEY, true)],
            step: Duration::from_millis(10),
        }
    );
}

#[test]
fn the_run_is_finished_after_the_last_game_frame_once_every_capture_arrived() {
    let mut script = WindowScript::new(&window(2, &[1], &[]));
    assert!(!script.finished());
    script.next(&tick(0, 10));
    assert!(!script.finished(), "one of two frames");
    script.next(&tick(1, 10));
    script.requested(3, 1);
    assert!(
        !script.finished(),
        "the capture of the last frame has not arrived"
    );

    script.delivered(3, &RetainedCaptureOutcome::Cancelled);

    assert!(script.finished());
    assert!(
        script.take_captures().is_empty(),
        "a cancelled capture keeps nothing"
    );
}

#[test]
fn a_cancelled_capture_releases_the_hold_without_keeping_a_frame() {
    let mut script = WindowScript::new(&window(4, &[0], &[]));
    script.next(&tick(0, 10));
    script.requested(5, 0);
    assert_eq!(script.next(&tick(1, 10)), Turn::Held);

    script.delivered(5, &RetainedCaptureOutcome::Cancelled);

    assert_eq!(frame_of(&script.next(&tick(2, 10))), Some(1));
    assert!(script.take_captures().is_empty());
}

#[test]
fn a_frame_is_asked_for_once_however_often_it_is_asked_about() {
    let mut script = WindowScript::new(&window(3, &[0], &[]));
    script.next(&tick(0, 10));
    assert_eq!(script.due_capture(), Some(0));
    script.requested(2, 0);
    assert_eq!(script.due_capture(), None);
}

#[test]
fn without_a_fixed_step_the_first_game_frame_takes_none_and_later_ones_the_time_the_window_took() {
    let mut open = window(4, &[0], &[]);
    open.fixed_dt = None;
    let mut script = WindowScript::new(&open);

    let first = script.next(&tick(0, 16));
    script.requested(1, 0);
    assert_eq!(script.next(&tick(1, 17)), Turn::Held);
    script.delivered(1, &RetainedCaptureOutcome::Cancelled);
    let second = script.next(&tick(2, 18));

    let step = |turn: &Turn| match turn {
        Turn::Advance { step, .. } => *step,
        Turn::Held => panic!("held"),
    };
    assert_eq!(step(&first), Duration::ZERO);
    assert_eq!(step(&second), Duration::from_millis(18));
}

#[test]
fn no_update_advances_the_game_past_the_last_frame_of_the_run() {
    let mut script = WindowScript::new(&window(2, &[], &[]));

    let turns: Vec<Turn> = (0..5)
        .map(|update| script.next(&tick(update, 10)))
        .collect();

    assert_eq!(
        turns.iter().map(frame_of).collect::<Vec<_>>(),
        [Some(0), Some(1), None, None, None]
    );
    assert_eq!(script.advanced(), 2);
}

#[test]
fn a_capture_that_has_not_arrived_in_the_limit_names_its_frame() {
    let mut script = WindowScript::new(&window(4, &[2], &[]));
    for update in 0..3 {
        script.next(&tick(update, 10));
    }
    assert_eq!(
        script.overdue(Duration::from_millis(1)),
        None,
        "none asked for yet"
    );
    script.requested(9, 2);
    assert_eq!(script.overdue(Duration::from_mins(5)), None, "not yet");

    std::thread::sleep(Duration::from_millis(3));

    assert_eq!(script.overdue(Duration::from_millis(1)), Some(2));
    script.delivered(9, &RetainedCaptureOutcome::Cancelled);
    assert_eq!(script.overdue(Duration::from_millis(1)), None, "it arrived");
}
