//! CPU-only: which changes make a TAA history bank stale.
use crate::history_key::HistoryKey;

const R: [u32; 2] = [640, 360];
const O: [u32; 2] = [1280, 720];

#[test]
fn native_history_ignores_the_presentation_extent() {
    assert_eq!(
        HistoryKey::new(R, None, 1),
        HistoryKey::new(R, Some(R), 1),
        "equal extents are the native stage"
    );
}

#[test]
fn native_history_follows_the_render_extent() {
    assert_ne!(
        HistoryKey::new(R, None, 1),
        HistoryKey::new([641, 360], None, 1)
    );
}

#[test]
fn temporal_history_is_replaced_by_a_render_only_resize() {
    assert_ne!(
        HistoryKey::new(R, Some(O), 1),
        HistoryKey::new([704, 396], Some(O), 1)
    );
}

#[test]
fn temporal_history_is_replaced_by_an_output_only_resize() {
    assert_ne!(
        HistoryKey::new(R, Some(O), 1),
        HistoryKey::new(R, Some([1920, 1080]), 1)
    );
}

#[test]
fn temporal_and_native_histories_are_never_interchangeable() {
    assert_ne!(HistoryKey::new(R, Some(O), 1), HistoryKey::new(R, None, 1));
}

#[test]
fn a_sample_count_change_replaces_the_history() {
    assert_ne!(HistoryKey::new(R, None, 1), HistoryKey::new(R, None, 4));
    assert_ne!(
        HistoryKey::new(R, Some(O), 1),
        HistoryKey::new(R, Some(O), 4)
    );
}

#[test]
fn an_unchanged_key_is_equal() {
    assert_eq!(
        HistoryKey::new(R, Some(O), 1),
        HistoryKey::new(R, Some(O), 1)
    );
}
