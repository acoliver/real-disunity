use super::*;

fn record(value: u8) -> Vec<u8> {
    vec![value; STRIDE]
}

fn prepare(history: &mut History, bytes: &[u8]) -> Candidate {
    history.prepare(bytes, &BTreeSet::new(), &Publications::default())
}

#[test]
fn upload_acknowledgement_does_not_advance_last_visible_transform() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    let refused = prepare(&mut history, &record(2));
    assert_eq!(refused.records[0].previous, [1; STRIDE]);
    drop(refused);
    let retry = prepare(&mut history, &record(2));
    assert_eq!(retry.records[0].previous, [1; STRIDE]);
    retry.publish();
    assert!(history.has_pending());
    let settling = prepare(&mut history, &record(2));
    assert_eq!(settling.records[0].previous, [2; STRIDE]);
    settling.publish();
    assert!(!history.has_pending());
    assert!(prepare(&mut history, &record(2)).records.is_empty());
}

#[test]
fn reset_survives_refusal_and_removed_candidates_cannot_publish_into_reused_slots() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    history.reset([0]);
    let abandoned = prepare(&mut history, &record(2));
    assert_eq!(abandoned.records[0].previous, [2; STRIDE]);
    drop(abandoned);
    let cancelled = prepare(&mut history, &record(2));
    history.remove(0);
    assert!(!history.has_pending());
    history.add(0);
    cancelled.publish();
    let reused = prepare(&mut history, &record(3));
    assert_eq!(reused.records[0].previous, [3; STRIDE]);
    reused.publish();
    assert!(!history.has_pending());
}

#[test]
fn history_frontier_remains_sparse_and_candidates_are_immutable() {
    let mut history = History::default();
    let mut bytes = vec![1; 1000 * STRIDE];
    for slot in 0..1000 {
        history.add(slot);
    }
    prepare(&mut history, &bytes).publish();
    history.edit(537);
    bytes[537 * STRIDE..538 * STRIDE].fill(2);
    let candidate = prepare(&mut history, &bytes);
    assert_eq!(candidate.records.len(), 1);
    assert_eq!(candidate.records[0].slot, 537);
    assert_eq!(candidate.records[0].previous, [1; STRIDE]);
    bytes[537 * STRIDE..538 * STRIDE].fill(3);
    candidate.publish();
    let next = prepare(&mut history, &bytes);
    assert_eq!(next.records[0].previous, [2; STRIDE]);
}

#[test]
fn repeated_visible_publication_cannot_reintroduce_a_settled_obligation() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    let motion = prepare(&mut history, &record(2));
    motion.clone().publish();
    prepare(&mut history, &record(2)).publish();
    motion.publish();
    assert!(!history.has_pending());
}

#[test]
fn same_frame_reuses_previous_operands_and_enlists_unrelated_new_slots() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    let owner = Publications::default();
    let mut bytes = record(2);
    let prefix = history.prepare(&bytes, &BTreeSet::new(), &owner);
    assert_eq!(
        history.previous_uploads(&prefix, &bytes, false),
        vec![(0, record(1))]
    );
    history.add(1);
    bytes.extend_from_slice(&record(3));
    let suffix = history.prepare(&bytes, &BTreeSet::new(), &owner);
    assert_eq!(suffix.records.len(), 2);
    assert_eq!(suffix.records[0].previous, [1; STRIDE]);
    assert_eq!(suffix.records[1].previous, [3; STRIDE]);
    assert_eq!(
        history.previous_uploads(&suffix, &bytes, false),
        vec![(STRIDE as u64, record(3))]
    );
    suffix.publish();
    let settled = prepare(&mut history, &bytes);
    assert_eq!(settled.records.len(), 1);
    assert_eq!(settled.records[0].previous, [2; STRIDE]);
}

#[test]
fn reverted_edits_keep_visible_candidates_without_unnecessary_previous_writes() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    let candidate = prepare(&mut history, &record(1));
    assert_eq!(candidate.records.len(), 1);
    assert!(
        history
            .previous_uploads(&candidate, &record(1), false)
            .is_empty()
    );
    candidate.publish();
    assert!(!history.has_pending());
}

#[test]
fn publication_keeps_the_drawn_snapshot_and_preserves_a_newer_edit() {
    let mut history = History::default();
    history.add(0);
    prepare(&mut history, &record(1)).publish();
    history.edit(0);
    let drawn = prepare(&mut history, &record(2));
    history.edit(0);
    drawn.publish();
    assert!(history.has_pending());
    let next = prepare(&mut history, &record(3));
    assert_eq!(next.records[0].previous, [2; STRIDE]);
    assert_eq!(next.records[0].current, [3; STRIDE]);
}

#[test]
fn publication_consumes_its_fresh_marker_but_preserves_a_newer_reset() {
    let mut history = History::default();
    history.add(0);
    let first = prepare(&mut history, &record(1));
    history.edit(0);
    first.publish();
    let moved = prepare(&mut history, &record(2));
    assert_eq!(moved.records[0].previous, [1; STRIDE]);
    history.reset([0]);
    moved.publish();
    assert!(history.has_pending());
    let reset = prepare(&mut history, &record(3));
    assert_eq!(reset.records[0].previous, [3; STRIDE]);
    reset.publish();
    assert!(!history.has_pending());
}

#[test]
fn physical_acknowledgement_admits_fresh_bytes_without_consuming_visible_debt() {
    let mut history = History::default();
    history.add(0);
    let fresh = prepare(&mut history, &record(1));
    assert!(!history.ready(&record(1), false, true));
    fresh.acknowledge_previous(true);
    assert!(history.ready(&record(1), false, true));
    assert!(history.has_pending());
    fresh.publish();
    history.edit(0);
    let motion = prepare(&mut history, &record(2));
    motion.acknowledge_previous(false);
    assert!(history.ready(&record(2), false, true));
    assert!(history.has_pending());
    motion.publish();
    assert!(!history.ready(&record(2), false, true));
    let settling = prepare(&mut history, &record(2));
    assert!(!history.ready(&record(2), false, true));
    settling.acknowledge_previous(false);
    assert!(history.ready(&record(2), false, true));
    assert!(history.has_pending());
    settling.publish();
    assert!(!history.has_pending());
}

#[test]
fn physical_readiness_rejects_new_revisions_resets_and_reused_generations() {
    let mut history = History::default();
    history.add(0);
    let first = prepare(&mut history, &record(1));
    first.acknowledge_previous(true);
    assert!(history.ready(&record(1), true, true));
    history.edit(0);
    assert!(!history.ready(&record(2), true, true));
    assert!(!history.ready(&record(2), false, true));
    let newer = prepare(&mut history, &record(2));
    history.reset([0]);
    assert!(!history.ready(&record(2), true, true));
    history.remove(0);
    history.add(0);
    newer.acknowledge_previous(true);
    assert!(!history.ready(&record(2), true, true));
    assert!(!history.ready(&record(2), false, true));
}
