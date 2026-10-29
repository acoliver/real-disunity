//! The depth-history disocclusion test and the linear-depth history on the GPU.
use super::*;
use crate::taa_reference::depth_tests::{device as device_depth, terms};

const SIZE: [i32; 2] = [17, 9];

/// Two depth layers split at x = 8, with a stored history that agrees on the left half and
/// holds the other layer on the right half.
fn case() -> Case {
    let mut case = Case::new(noisy(SIZE, 0), noisy(SIZE, 5));
    case.depth = Depth::from_fn(SIZE, |x, _| device_depth(if x < 8 { 5.0 } else { 20.0 }));
    case.depth_history = Some(Depth::from_fn(SIZE, |x, _| if x < 8 { 5.05 } else { 5.0 }));
    case.depth_terms = terms();
    case.jitter = [0.25, -0.125];
    case
}

#[test]
fn the_disocclusion_test_matches_the_reference_on_synthetic_depth_history() {
    let case = case();
    let out = assert_matches_reference(&case, "depth history");
    // The right half takes the current colour, the left half still blends with history.
    let mut off = case_without_test();
    off.valid = true;
    let kept = off.expected();
    let differs =
        |x: usize| (0..9).any(|y| (out[y * 17 + x][0] - kept[y * 17 + x][0]).abs() > 1e-3);
    assert!(differs(12), "the right half ignores history");
    assert!(!differs(3), "the left half is unchanged");
}

fn case_without_test() -> Case {
    let mut off = case();
    off.depth_terms = [0.0; 2];
    off
}

#[test]
fn the_resolve_stores_the_linear_depth_of_the_closest_neighbour_for_the_next_frame() {
    let mut case = case();
    case.valid = false;
    let (_, _, stored) = run_with_depth(&case);
    let history = case.quantised_history();
    let frame = Frame {
        color: &case.color,
        depth: &case.depth,
        history: &history,
        reprojection: case.reprojection,
        jitter: case.jitter.map(f64::from),
        history_valid: false,
        settings: case.settings,
        samples: 1,
        velocity: None,
        depth_history: None,
        depth_terms: case.depth_terms,
    };
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let index = usize::try_from(y * SIZE[0] + x).unwrap();
            let expected = frame.stored_depth([x, y]);
            let actual = f64::from(stored[index]);
            assert!(
                (actual - expected).abs() < 1e-3 * expected,
                "{x},{y}: {actual} {expected}"
            );
        }
    }
    assert!(stored.iter().any(|d| (*d - 5.0).abs() < 0.01));
    assert!(stored.iter().any(|d| (*d - 20.0).abs() < 0.05));
}

#[test]
fn without_depth_terms_the_resolve_ignores_the_depth_history() {
    let mut with_zero_history = case_without_test();
    with_zero_history.depth_history = None;
    let (a, ..) = run(&with_zero_history);
    let (b, ..) = run(&case_without_test());
    assert_eq!(a, b);
}
