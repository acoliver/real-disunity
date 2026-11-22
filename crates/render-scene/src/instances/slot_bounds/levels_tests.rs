//! The level memo gives every slot the outcome of the current eye, computing it again only when
//! the eye has travelled beyond the margin of what it stored.
use std::cell::Cell;

use super::*;

fn eye(index: i64) -> LevelKey {
    LevelKey {
        eye_millimeters: [index, 0, 0],
        fade_eye: [0; 3],
        fade_cull_disabled: false,
    }
}

/// How far from a multiple of 100 a slot's eye coordinate lies decides its outcome: each hundred
/// millimetres of a slot's own axis is one band, and bands cycle through the outcomes.
fn outcome(eye: i64, slot: usize) -> Computed {
    let shifted = eye + i64::try_from(slot).unwrap() * 37;
    let band = shifted.div_euclid(100);
    let level = match band.rem_euclid(4) {
        0 => Level::Faded,
        lod => Level::Drawn(usize::try_from(lod - 1).unwrap()),
    };
    // Whole millimetres to either edge of the band, less the one a rounded square root costs.
    let into = shifted.rem_euclid(100);
    let margin = u64::try_from((into.min(99 - into) - 1).max(0)).unwrap();
    Computed { level, margin }
}

/// A memo and the path of the eye that tells its entries apart, driven as a frame's selection
/// drives them.
#[derive(Default)]
struct Memo {
    levels: Levels,
    path: EyePath,
    travelled: u64,
}

impl Memo {
    fn prepare(&mut self, key: LevelKey, slots: usize) {
        let (travelled, restarted) = self.path.advance(key);
        if restarted {
            self.levels.clear();
        }
        self.levels.grow(slots);
        self.travelled = travelled;
    }

    fn forget(&mut self, slot: usize) {
        self.levels.forget(slot);
    }
}

fn ask(memo: &Memo, eye: i64, slot: usize, computed: &Cell<usize>) -> Level {
    memo.levels.get_or_compute(memo.travelled, slot, || {
        computed.set(computed.get() + 1);
        Computed {
            margin: 0,
            ..outcome(eye, slot)
        }
    })
}

fn ask_with_margin(memo: &Memo, eye: i64, slot: usize, computed: &Cell<usize>) -> Level {
    memo.levels.get_or_compute(memo.travelled, slot, || {
        computed.set(computed.get() + 1);
        outcome(eye, slot)
    })
}

#[test]
fn a_slot_without_margin_is_computed_once_per_eye_and_again_for_a_new_eye() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    memo.prepare(eye(1), 8);
    for _ in 0..3 {
        for slot in 0..8 {
            assert_eq!(ask(&memo, 1, slot, &computed), outcome(1, slot).level);
        }
    }
    assert_eq!(computed.get(), 8);
    memo.prepare(eye(1), 8);
    ask(&memo, 1, 3, &computed);
    assert_eq!(computed.get(), 8, "the same eye keeps its entries");
    memo.prepare(eye(2), 8);
    for slot in 0..8 {
        assert_eq!(ask(&memo, 2, slot, &computed), outcome(2, slot).level);
    }
    assert_eq!(computed.get(), 16);
    memo.prepare(eye(1), 8);
    for slot in 0..8 {
        assert_eq!(ask(&memo, 1, slot, &computed), outcome(1, slot).level);
    }
    assert_eq!(
        computed.get(),
        24,
        "returning to an earlier eye is a new eye"
    );
}

#[test]
fn a_new_fade_eye_at_the_same_place_is_a_new_eye() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    memo.prepare(eye(1), 2);
    ask(&memo, 1, 0, &computed);
    let mut moved = eye(1);
    moved.fade_eye = [1; 3];
    memo.prepare(moved, 2);
    ask(&memo, 1, 0, &computed);
    let mut disabled = moved;
    disabled.fade_cull_disabled = true;
    memo.prepare(disabled, 2);
    ask(&memo, 1, 0, &computed);
    assert_eq!(computed.get(), 3);
}

#[test]
fn an_entry_holds_exactly_while_the_eye_has_travelled_no_farther_than_its_margin() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    // Slot 0 at eye 10 lies in a band with 10 whole millimetres to its low edge and 89 to its
    // high edge; the margin keeps one back.
    memo.prepare(eye(10), 1);
    let margin = outcome(10, 0).margin;
    assert_eq!(margin, 9);
    let first = ask_with_margin(&memo, 10, 0, &computed);
    for travelled in 1..=margin {
        memo.prepare(eye(10 - i64::try_from(travelled).unwrap()), 1);
        assert_eq!(ask_with_margin(&memo, 10, 0, &computed), first);
    }
    assert_eq!(
        computed.get(),
        1,
        "an eye within the margin reuses the entry"
    );
    memo.prepare(eye(0), 1);
    ask_with_margin(&memo, 0, 0, &computed);
    assert_eq!(
        computed.get(),
        2,
        "one millimetre past the margin recomputes"
    );
}

#[test]
fn travel_counts_the_whole_path_not_the_distance_from_the_start() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    memo.prepare(eye(10), 1);
    ask_with_margin(&memo, 10, 0, &computed);
    // Out and back by four millimetres each way spends eight of the margin of nine.
    for step in [14, 10, 11] {
        memo.prepare(eye(step), 1);
        ask_with_margin(&memo, step, 0, &computed);
    }
    assert_eq!(
        computed.get(),
        1,
        "nine millimetres of travel are within the margin"
    );
    memo.prepare(eye(10), 1);
    ask_with_margin(&memo, 10, 0, &computed);
    assert_eq!(
        computed.get(),
        2,
        "the tenth recomputes, though the eye is where it began"
    );
}

#[test]
fn a_forgotten_slot_and_a_grown_memo_compute_only_what_is_new() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    memo.prepare(eye(5), 4);
    for slot in 0..4 {
        ask_with_margin(&memo, 5, slot, &computed);
    }
    memo.forget(2);
    memo.forget(40);
    memo.prepare(eye(5), 6);
    for slot in 0..6 {
        assert_eq!(
            ask_with_margin(&memo, 5, slot, &computed),
            outcome(5, slot).level
        );
    }
    assert_eq!(
        computed.get(),
        4 + 1 + 2,
        "slot 2 again, and the new slots 4 and 5"
    );
}

#[test]
fn the_memo_agrees_with_computing_every_time_across_walks_jumps_and_forgotten_slots() {
    let mut memo = Memo::default();
    let computed = Cell::new(0);
    let mut state = 99_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap()
    };
    let slots = 30;
    let mut at = 0_i64;
    let mut asked = 0;
    for step in 0..2_000 {
        at += match next() % 50 {
            0 => 1_000_000_007,
            1..=5 => -i64::try_from(next() % 400).unwrap(),
            _ => i64::try_from(next() % 12).unwrap(),
        };
        memo.prepare(eye(at), slots);
        for _ in 0..40 {
            let slot = next() % slots;
            if next() % 11 == 0 {
                memo.forget(slot);
            }
            assert_eq!(
                ask_with_margin(&memo, at, slot, &computed),
                outcome(at, slot).level,
                "step {step} at {at}"
            );
            asked += 1;
        }
    }
    assert!(
        computed.get() * 3 < asked,
        "walking eyes reuse most entries: {} computed of {asked}",
        computed.get()
    );
}

#[test]
fn the_path_grows_by_the_length_of_each_move_rounded_up_and_by_one_at_least() {
    let key = |eye_millimeters| LevelKey {
        eye_millimeters,
        ..eye(0)
    };
    let mut path = EyePath::default();
    let mut travelled = path.advance(key([0; 3])).0;
    for (to, grows) in [
        ([3, 4, 0], 5),
        ([3, 4, 0], 0),
        ([4, 5, 1], 2),
        ([-4, 5, 1], 8),
        ([-3, 5, 1], 1),
    ] {
        let (now, restarted) = path.advance(key(to));
        assert!(!restarted);
        assert_eq!(now - travelled, grows, "move to {to:?}");
        travelled = now;
    }
    let mut changed = key([-3, 5, 1]);
    changed.fade_eye = [7; 3];
    assert_eq!(path.advance(changed).0, travelled + 1);
}

#[test]
fn a_move_too_long_to_count_restarts_the_path_and_says_so() {
    let key = |x| LevelKey {
        eye_millimeters: [x, 0, 0],
        ..eye(0)
    };
    let mut path = EyePath::default();
    path.advance(key(0));
    assert_eq!(path.advance(key(i64::MAX)), (0, true));
    assert_eq!(path.advance(key(i64::MIN)), (0, true));
    assert_eq!(path.advance(key(i64::MIN + 5)), (5, false));
    // Many long moves that each fit still end in a restart before the travel overflows.
    let mut restarts = 0;
    for step in 0..40_i64 {
        let (travelled, restarted) = path.advance(key(i64::MIN + 5 + (step % 2) * (1 << 50)));
        restarts += usize::from(restarted);
        assert!(travelled <= MAX_TRAVEL);
    }
    assert!(restarts > 0);
}
