use super::*;

const BOTH: Halves = Halves {
    previous: true,
    skin: false,
};
const CURRENT: Halves = Halves {
    previous: false,
    skin: false,
};

fn keys(batches: &[usize]) -> Vec<BatchKey> {
    batches.iter().map(|&batch| (0, 0, batch)).collect()
}

#[test]
fn a_state_that_remembers_nothing_reports_every_key() {
    let verified = Verified::default();
    assert_eq!(
        verified.missing(7, CURRENT, &keys(&[1, 2, 3])),
        keys(&[1, 2, 3])
    );
    assert!(verified.missing(7, CURRENT, &[]).is_empty());
}

#[test]
fn a_remembered_key_is_not_reported_again_but_a_new_one_is() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 3, 5]));
    assert!(verified.missing(7, CURRENT, &keys(&[1, 3, 5])).is_empty());
    assert!(verified.missing(7, CURRENT, &keys(&[3, 5])).is_empty());
    assert_eq!(
        verified.missing(7, CURRENT, &keys(&[0, 1, 2, 3, 4, 5, 6])),
        keys(&[0, 2, 4, 6])
    );
}

#[test]
fn remembering_more_keys_at_the_same_figures_keeps_the_earlier_ones() {
    let verified = Verified::default();
    verified.remember(7, BOTH, &keys(&[1, 5]));
    verified.remember(7, BOTH, &keys(&[3, 5, 8]));
    assert!(verified.missing(7, BOTH, &keys(&[1, 3, 5, 8])).is_empty());
}

#[test]
fn other_content_or_other_halves_leave_nothing_remembered() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 2]));
    assert_eq!(verified.missing(8, CURRENT, &keys(&[1, 2])), keys(&[1, 2]));
    assert_eq!(verified.missing(7, BOTH, &keys(&[1, 2])), keys(&[1, 2]));
    // New figures replace the old ones instead of adding to them.
    verified.remember(8, CURRENT, &keys(&[4]));
    assert_eq!(verified.missing(7, CURRENT, &keys(&[1, 2])), keys(&[1, 2]));
    assert!(verified.missing(8, CURRENT, &keys(&[4])).is_empty());
}

#[test]
fn a_forgotten_key_is_reported_and_the_others_stay_remembered() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 2, 3]));
    verified.forget((0, 0, 2));
    verified.forget((0, 0, 9));
    assert_eq!(verified.missing(7, CURRENT, &keys(&[1, 2, 3])), keys(&[2]));
}

#[test]
fn a_copy_remembers_what_the_original_did_and_changes_alone() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 2]));
    let copy = verified.clone();
    copy.forget((0, 0, 1));
    copy.remember(7, CURRENT, &keys(&[6]));
    assert!(verified.missing(7, CURRENT, &keys(&[1, 2])).is_empty());
    assert_eq!(verified.missing(7, CURRENT, &keys(&[6])), keys(&[6]));
    assert_eq!(copy.missing(7, CURRENT, &keys(&[1, 2, 6])), keys(&[1]));
}

#[test]
fn a_list_out_of_order_is_only_rechecked() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 2, 3]));
    let missing = verified.missing(7, CURRENT, &keys(&[3, 1, 9]));
    assert!(missing.contains(&(0, 0, 9)), "{missing:?}");
}

#[test]
fn new_figures_do_not_keep_the_keys_remembered_at_the_old_ones() {
    let verified = Verified::default();
    verified.remember(7, CURRENT, &keys(&[1, 2]));
    verified.remember(8, CURRENT, &keys(&[4]));
    assert_eq!(
        verified.missing(8, CURRENT, &keys(&[1, 2, 4])),
        keys(&[1, 2])
    );
    verified.remember(8, BOTH, &keys(&[5]));
    assert_eq!(verified.missing(8, BOTH, &keys(&[4, 5])), keys(&[4]));
}
