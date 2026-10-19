use std::mem::{align_of, offset_of, size_of};

use worldrunner_disunity_ffi::{DisunityCommand, DisunityObjectState, DisunityResult};

#[test]
fn result_layout_matches_the_managed_contract() {
    assert_eq!(size_of::<DisunityResult>(), 16);
    assert_eq!(align_of::<DisunityResult>(), 8);
    assert_eq!(offset_of!(DisunityResult, code), 0);
    assert_eq!(offset_of!(DisunityResult, detail), 8);
}

#[test]
fn command_layout_matches_the_managed_contract() {
    assert_eq!(size_of::<DisunityCommand>(), 24);
    assert_eq!(align_of::<DisunityCommand>(), 8);
    assert_eq!(offset_of!(DisunityCommand, object), 0);
    assert_eq!(offset_of!(DisunityCommand, kind), 8);
    assert_eq!(offset_of!(DisunityCommand, x), 12);
    assert_eq!(offset_of!(DisunityCommand, y), 16);
    assert_eq!(offset_of!(DisunityCommand, z), 20);
}

#[test]
fn object_state_layout_matches_the_managed_contract() {
    assert_eq!(size_of::<DisunityObjectState>(), 32);
    assert_eq!(align_of::<DisunityObjectState>(), 8);
    assert_eq!(offset_of!(DisunityObjectState, object), 0);
    assert_eq!(offset_of!(DisunityObjectState, x), 8);
    assert_eq!(offset_of!(DisunityObjectState, y), 12);
    assert_eq!(offset_of!(DisunityObjectState, z), 16);
    assert_eq!(offset_of!(DisunityObjectState, rotation_y_radians), 20);
    assert_eq!(offset_of!(DisunityObjectState, material_slot), 24);
}
