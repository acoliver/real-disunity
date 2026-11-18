//! A contour pass reads the opaque depth, so a frozen plan lists it after every Opaque entry that
//! writes that depth. The order rule names the contour and the Opaque entry it must follow; it
//! does not depend on whether a depth prepass completed the depth. Every refusal comes before any
//! feature prepares or draws.
use super::{FrameError, PassEntry, PassSlot, Step, encode_order, refused};
use Step::*;

/// The occurrences of the contour slot a plan lists.
const OCCURRENCES: [u32; 2] = [0, 1];

/// The first listed Opaque entry after the first contour is the one the contour must follow.
fn assert_contour_refused(steps: &[Step]) {
    let (order, outcome) = encode_order(steps, false);
    let at = steps
        .iter()
        .position(|step| matches!(step, Contour(_)))
        .expect("a plan with a contour");
    let needs = order.entries[at + 1..]
        .iter()
        .find(|entry| entry.slot() == PassSlot::Opaque)
        .expect("an Opaque entry listed after the contour");
    refused(
        &outcome,
        FrameError::StockOrder {
            pass: order.entries[at],
            needs: *needs,
        },
    );
}

fn assert_admitted_as_listed(steps: &[Step]) {
    let (order, outcome) = encode_order(steps, false);
    outcome.result.unwrap();
    assert_eq!(outcome.prepared, 1);
    assert_eq!(outcome.draws, order.entries);
}

#[test]
fn a_contour_listed_before_opaque_is_refused_even_after_a_matching_prepass() {
    for occurrence in OCCURRENCES {
        assert_contour_refused(&[Prepass, Contour(occurrence), Opaque]);
        assert_contour_refused(&[Prepass, Contour(occurrence), Opaque, Opaque]);
        assert_contour_refused(&[Contour(occurrence), Opaque]);
    }
}

#[test]
fn the_refusal_of_an_early_contour_names_its_occurrence_and_the_first_opaque_entry() {
    let (_, outcome) = encode_order(&[Prepass, Contour(1), Opaque, Opaque], false);
    refused(
        &outcome,
        FrameError::StockOrder {
            pass: PassEntry::nth(PassSlot::ContourIds, 1),
            needs: PassEntry::new(PassSlot::Opaque),
        },
    );
}

#[test]
fn a_contour_listed_after_every_opaque_entry_is_admitted_in_the_order_given() {
    for occurrence in OCCURRENCES {
        assert_admitted_as_listed(&[Opaque, Contour(occurrence)]);
        assert_admitted_as_listed(&[Prepass, Opaque, Contour(occurrence)]);
        assert_admitted_as_listed(&[Opaque, Opaque, Contour(occurrence)]);
        assert_admitted_as_listed(&[Prepass, Opaque, Opaque, Contour(occurrence)]);
    }
    assert_admitted_as_listed(&[Opaque, Contour(1), Contour(0)]);
}

#[test]
fn a_contour_between_repeated_opaque_entries_is_refused_by_the_order_rule() {
    for occurrence in OCCURRENCES {
        assert_contour_refused(&[Opaque, Contour(occurrence), Opaque]);
        assert_contour_refused(&[Prepass, Opaque, Contour(occurrence), Opaque]);
        assert_contour_refused(&[Opaque, Opaque, Contour(occurrence), Opaque]);
    }
}

/// A custom reader between Opaque writers, with a contour correctly after them all.
fn assert_reader_incomplete(steps: &[Step], lists_depth: bool) {
    let (order, outcome) = encode_order(steps, lists_depth);
    let reader = steps
        .iter()
        .position(|step| matches!(step, Reader))
        .expect("a plan with a reader");
    refused(
        &outcome,
        FrameError::IncompleteDepth {
            pass: order.entries[reader],
            resource: order.depth,
        },
    );
}

#[test]
fn a_custom_depth_reader_between_opaque_entries_stays_refused_with_a_contour_after_them() {
    for lists_depth in [false, true] {
        assert_reader_incomplete(&[Opaque, Reader, Opaque, Contour(0)], lists_depth);
    }
}

#[test]
fn a_prepass_of_other_storage_does_not_complete_the_depth_for_a_custom_reader_before_a_contour() {
    for lists_depth in [false, true] {
        assert_reader_incomplete(
            &[OtherPrepass, Opaque, Reader, Opaque, Contour(0)],
            lists_depth,
        );
    }
}

#[test]
fn a_custom_depth_reader_after_a_matching_prepass_is_admitted_with_a_contour_after_opaque() {
    for lists_depth in [false, true] {
        let (order, outcome) =
            encode_order(&[Prepass, Opaque, Reader, Opaque, Contour(0)], lists_depth);
        outcome.result.unwrap();
        assert_eq!(outcome.prepared, 1);
        assert_eq!(outcome.draws, order.entries);
    }
}
