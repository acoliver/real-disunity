//! Which frames show the game's screen: a write counts once the frame that carried it was
//! submitted, a new allocation starts empty, and a screen nobody wrote is never shown.

use super::{Ledger, Show};

const FIRST: u32 = 1;
const SECOND: u32 = 2;

fn shown(screen: &u32, retained: bool) -> Show<'_, u32> {
    Show { screen, retained }
}

#[test]
fn without_an_offered_screen_every_frame_shows_the_stock_scene() {
    let mut ledger = Ledger::<u32>::default();

    assert_eq!(ledger.plan(false), None);
    assert_eq!(ledger.plan(true), None, "nothing to show it with");
}

#[test]
fn a_screen_written_in_this_frame_is_shown_in_this_frame_and_retained_after() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, false)));

    assert_eq!(ledger.plan(true), Some(shown(&FIRST, false)));
    ledger.submitted();

    ledger.offer(Some((&FIRST, false)));
    assert_eq!(
        ledger.plan(false),
        Some(shown(&FIRST, true)),
        "the frame after it has no writer, the earlier submission's contents are shown"
    );
}

#[test]
fn an_unwritten_screen_is_not_shown() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, false)));

    assert_eq!(ledger.plan(false), None);
    ledger.submitted();
    ledger.offer(Some((&FIRST, false)));
    assert_eq!(
        ledger.plan(false),
        None,
        "submitting a frame that did not write it wrote nothing"
    );
}

#[test]
fn a_writer_whose_frame_was_not_submitted_leaves_the_screen_unwritten() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, false)));
    assert_eq!(ledger.plan(true), Some(shown(&FIRST, false)));
    // The frame failed to encode or was dropped: `submitted` is never called for it.

    ledger.offer(Some((&FIRST, false)));
    assert_eq!(ledger.plan(false), None, "no retained view was produced");
    ledger.submitted();
    ledger.offer(Some((&FIRST, false)));
    assert_eq!(
        ledger.plan(false),
        None,
        "a later submission does not count the dropped write"
    );
}

#[test]
fn a_write_survives_a_later_frame_that_has_no_writer() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, false)));
    ledger.plan(true);
    ledger.submitted();

    for _ in 0..3 {
        ledger.offer(Some((&FIRST, false)));
        assert_eq!(ledger.plan(false), Some(shown(&FIRST, true)));
        ledger.submitted();
    }
}

#[test]
fn contents_the_host_submitted_before_any_frame_are_shown_without_a_writer() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, true)));

    assert_eq!(ledger.plan(false), Some(shown(&FIRST, true)));
}

#[test]
fn another_allocation_starts_empty_and_a_pending_write_does_not_carry_over() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, false)));
    ledger.plan(true);
    ledger.submitted();
    ledger.offer(Some((&FIRST, false)));
    assert_eq!(ledger.plan(true), Some(shown(&FIRST, true)));

    ledger.offer(Some((&SECOND, false)));
    ledger.submitted();

    assert_eq!(
        ledger.plan(false),
        None,
        "the resized screen was never written"
    );
    ledger.offer(Some((&SECOND, false)));
    assert_eq!(ledger.plan(true), Some(shown(&SECOND, false)));
}

#[test]
fn withdrawing_the_screen_forgets_its_contents() {
    let mut ledger = Ledger::default();
    ledger.offer(Some((&FIRST, true)));
    assert_eq!(ledger.plan(false), Some(shown(&FIRST, true)));

    ledger.offer(None);
    assert_eq!(ledger.plan(true), None);

    ledger.offer(Some((&FIRST, false)));
    assert_eq!(ledger.plan(false), None, "offered again, it starts empty");
}

#[test]
fn capture_preflight_predicts_selection_without_recording_a_write() {
    let mut ledger = Ledger::<u32>::default();
    for offered in [
        None,
        Some((&FIRST, false)),
        Some((&FIRST, true)),
        Some((&SECOND, false)),
    ] {
        ledger.offer(offered);
        let written = ledger.has_submitted_screen();
        for exported in [true, false] {
            let expected = offered.is_some() && (written || exported);
            assert_eq!(ledger.will_show(exported), expected);
            ledger.submitted();
            assert_eq!(ledger.has_submitted_screen(), written);
            assert_eq!(ledger.plan(exported).is_some(), expected);
            ledger.offer(offered);
        }
    }
}

#[test]
fn the_sampling_pass_is_invalidated_only_when_the_offered_allocation_changes() {
    let mut ledger = Ledger::default();
    assert!(!ledger.offer(None));
    assert!(ledger.offer(Some((&FIRST, false))));
    assert!(!ledger.offer(Some((&FIRST, true))));
    assert!(ledger.offer(Some((&SECOND, false))));
    assert!(!ledger.offer(Some((&SECOND, false))));
    assert!(ledger.offer(None));
    assert!(!ledger.offer(None));
}
