use std::ops::Range;

use super::plan::Plan;
use crate::draw::visibility::SelectedRange;
use crate::instances::BatchKey;

const A: BatchKey = (0, 0, 0);
const B: BatchKey = (0, 0, 2);

fn range(key: BatchKey, lod: usize, slots: Range<u32>) -> SelectedRange {
    SelectedRange { key, lod, slots }
}

#[test]
fn groups_follow_batch_then_lod_with_slot_order_inside_a_group() {
    let plan = Plan::new(&[
        range(A, 0, 0..2),
        range(A, 1, 2..3),
        range(A, 0, 4..6),
        range(B, 0, 0..1),
    ])
    .unwrap();
    let groups: Vec<_> = plan
        .groups
        .iter()
        .map(|group| (group.key, group.lod, group.slots.clone()))
        .collect();
    assert_eq!(groups, [(A, 0, 0..4), (A, 1, 4..5), (B, 0, 5..6)]);
    assert_eq!(plan.total, 6);
}

#[test]
fn every_selected_slot_is_copied_once_to_its_group_position() {
    let plan = Plan::new(&[
        range(A, 0, 0..2),
        range(A, 1, 2..3),
        range(A, 0, 4..6),
        range(B, 0, 0..1),
    ])
    .unwrap();
    let copies: Vec<_> = plan
        .copies
        .iter()
        .map(|copy| (copy.key, copy.source.clone(), copy.destination))
        .collect();
    assert_eq!(
        copies,
        [(A, 0..2, 0), (A, 4..6, 2), (A, 2..3, 4), (B, 0..1, 5)]
    );
}

#[test]
fn an_empty_selection_has_no_groups_and_no_copies() {
    let plan = Plan::new(&[]).unwrap();
    assert!(plan.groups.is_empty() && plan.copies.is_empty());
    assert_eq!(plan.total, 0);
}

#[test]
fn a_selection_beyond_the_draw_range_is_not_compacted() {
    assert!(Plan::new(&[range(A, 0, 0..u32::MAX), range(B, 0, 0..2)]).is_none());
}
