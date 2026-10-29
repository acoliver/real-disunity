//! The slot order the shared outputs rely on: a resolve keeps its history only when the
//! previous resolve was the one of the preceding slot.
use super::SharedOutputs;
use crate::{TAA_UNIFORM_BYTES, TaaUniform};

fn block(history_valid: bool) -> [u8; TAA_UNIFORM_BYTES] {
    TaaUniform {
        reprojection: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        jitter_pixels: [0.25, -0.5],
        history_valid,
    }
    .to_bytes()
}

fn acquired() -> SharedOutputs {
    let (device, _) = crate::gpu_tests::gpu();
    let outputs = SharedOutputs::default();
    for slot in 0..super::SLOTS {
        outputs.acquire(&device, [4, 4], slot);
    }
    outputs
}

#[test]
fn clearing_the_flag_changes_nothing_else_in_the_block() {
    assert_eq!(crate::taa::without_history(block(true)), block(false));
    assert_eq!(crate::taa::without_history(block(false)), block(false));
}

#[test]
fn a_rotation_through_the_slots_keeps_history_after_the_first_frame() {
    let outputs = acquired();
    assert_eq!(outputs.admit(0, Some(block(true))), Some(block(false)));
    for slot in [1, 2, 0, 1, 2, 0] {
        assert_eq!(
            outputs.admit(slot, Some(block(true))),
            Some(block(true)),
            "{slot}"
        );
    }
}

#[test]
fn a_repeated_slot_loses_history() {
    let outputs = acquired();
    outputs.admit(0, None);
    outputs.admit(1, Some(block(true)));
    assert_eq!(outputs.admit(1, Some(block(true))), Some(block(false)));
}

#[test]
fn a_skipped_slot_loses_history_once_and_the_rotation_then_recovers() {
    let outputs = acquired();
    outputs.admit(0, None);
    assert_eq!(outputs.admit(2, Some(block(true))), Some(block(false)));
    assert_eq!(outputs.admit(0, Some(block(true))), Some(block(true)));
}

#[test]
fn a_frame_without_a_block_still_counts_as_the_last_resolve() {
    let outputs = acquired();
    outputs.admit(0, None);
    assert_eq!(outputs.admit(1, None), None);
    assert_eq!(outputs.admit(2, Some(block(true))), Some(block(true)));
}

#[test]
fn new_outputs_after_a_resize_start_without_history() {
    let (device, _) = crate::gpu_tests::gpu();
    let outputs = acquired();
    outputs.admit(0, None);
    outputs.acquire(&device, [8, 2], 1);
    assert_eq!(outputs.admit(1, Some(block(true))), Some(block(false)));
}
