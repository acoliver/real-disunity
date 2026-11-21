//! The level memo gives every slot the outcome computed for the current eye, computing it at
//! most once per eye, however many eyes it has seen.
use std::cell::Cell;

use super::*;

fn eye(index: i64) -> LevelKey {
    LevelKey {
        eye_millimeters: [index, 0, 0],
        fade_eye: [0; 3],
        fade_cull_disabled: false,
    }
}

/// What a slot draws at for an eye in these tests.
fn outcome(eye: i64, slot: usize) -> Level {
    match (usize::try_from(eye).unwrap() * 7 + slot) % 4 {
        0 => Level::Faded,
        lod => Level::Drawn(lod - 1),
    }
}

fn ask(levels: &Levels, eye: i64, slot: usize, computed: &Cell<usize>) -> Level {
    levels.get_or_compute(slot, || {
        computed.set(computed.get() + 1);
        outcome(eye, slot)
    })
}

#[test]
fn a_slot_is_computed_once_per_eye_and_again_for_a_new_eye() {
    let mut levels = Levels::default();
    let computed = Cell::new(0);
    levels.prepare(eye(1), 8);
    for _ in 0..3 {
        for slot in 0..8 {
            assert_eq!(ask(&levels, 1, slot, &computed), outcome(1, slot));
        }
    }
    assert_eq!(computed.get(), 8);
    levels.prepare(eye(1), 8);
    ask(&levels, 1, 3, &computed);
    assert_eq!(computed.get(), 8, "the same eye keeps its entries");
    levels.prepare(eye(2), 8);
    for slot in 0..8 {
        assert_eq!(ask(&levels, 2, slot, &computed), outcome(2, slot));
    }
    assert_eq!(computed.get(), 16);
    levels.prepare(eye(1), 8);
    for slot in 0..8 {
        assert_eq!(ask(&levels, 1, slot, &computed), outcome(1, slot));
    }
    assert_eq!(
        computed.get(),
        24,
        "returning to an earlier eye is a new eye"
    );
}

#[test]
fn a_forgotten_slot_and_a_grown_memo_compute_only_what_is_new() {
    let mut levels = Levels::default();
    let computed = Cell::new(0);
    levels.prepare(eye(5), 4);
    for slot in 0..4 {
        ask(&levels, 5, slot, &computed);
    }
    levels.forget(2);
    levels.forget(40);
    levels.prepare(eye(5), 6);
    for slot in 0..6 {
        assert_eq!(ask(&levels, 5, slot, &computed), outcome(5, slot));
    }
    assert_eq!(
        computed.get(),
        4 + 1 + 2,
        "slot 2 again, and the new slots 4 and 5"
    );
}

#[test]
fn the_memo_agrees_with_computing_every_time_across_more_eyes_than_one_epoch_counts() {
    let mut levels = Levels::default();
    let computed = Cell::new(0);
    let mut state = 99_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(state >> 33).unwrap()
    };
    let slots = 30;
    for step in 0..700_i64 {
        levels.prepare(eye(step), slots);
        let mut seen = [false; 30];
        let before = computed.get();
        for _ in 0..80 {
            let slot = next() % slots;
            if next() % 11 == 0 {
                levels.forget(slot);
                seen[slot] = false;
            }
            assert_eq!(
                ask(&levels, step, slot, &computed),
                outcome(step, slot),
                "eye {step}"
            );
            seen[slot] = true;
        }
        assert!(computed.get() - before >= seen.iter().filter(|&&s| s).count());
    }
    // A forgotten slot is the only reason to compute one twice in an eye.
    assert!(computed.get() < 700 * 80);
}

#[test]
fn an_entry_of_an_earlier_epoch_is_never_taken_for_the_current_one() {
    let mut levels = Levels::default();
    let computed = Cell::new(0);
    levels.prepare(eye(0), 1);
    ask(&levels, 0, 0, &computed);
    // Exactly one full cycle of epochs later the stored entry carries a recycled epoch.
    for step in 1..=255 {
        levels.prepare(eye(step), 1);
    }
    let before = computed.get();
    assert_eq!(ask(&levels, 255, 0, &computed), outcome(255, 0));
    assert_eq!(computed.get(), before + 1);
}
