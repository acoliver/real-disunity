//! What a test may assume of a frame's pass timings.
//!
//! `PassTiming::duration_ns` is `None` for a counter pair the adapter did not fill in. On Metal
//! the pair of the last pass stamped in a frame has been seen unusable in some frames, and no
//! earlier pass has (hub issue 9f99236ea41c). A test that reads timings therefore requires a
//! duration of every pass except the last one stamped, and reports the passes without one by
//! name. `FrameTiming::passes` is in stamping order; `timing_tests` asserts that order for a
//! chain whose passes it controls.

use crate::{FrameTiming, PassId};

/// The durations of `frame`'s passes in the order they were stamped, after printing how many
/// passes have one and which do not.
///
/// # Panics
/// Panics when a pass other than the last one stamped has no duration.
pub(crate) fn checked_durations(label: &str, frame: &FrameTiming) -> Vec<(PassId, Option<f64>)> {
    let untimed: Vec<_> = frame
        .passes
        .iter()
        .filter(|pass| pass.duration_ns.is_none())
        .map(|pass| pass.pass_id)
        .collect();
    eprintln!(
        "{label} frame={}: {} of {} passes timed; without a duration: {untimed:?}",
        frame.frame_id,
        frame.passes.len() - untimed.len(),
        frame.passes.len(),
    );
    let last = frame.passes.len().saturating_sub(1);
    for (index, pass) in frame.passes.iter().enumerate() {
        assert!(
            index == last || pass.duration_ns.is_some(),
            "{label} frame={}: {:?}, pass {index} of {}, has no duration and is not the last pass stamped",
            frame.frame_id,
            pass.pass_id,
            frame.passes.len(),
        );
    }
    frame
        .passes
        .iter()
        .map(|pass| (pass.pass_id, pass.duration_ns))
        .collect()
}

/// The total of the available measurements, or no measurement.
pub(crate) fn total(durations: impl Iterator<Item = f64>) -> Option<f64> {
    durations.reduce(|sum, ns| sum + ns)
}

#[test]
fn a_total_distinguishes_unavailable_measurements_from_measured_zero() {
    assert_eq!(total([].into_iter()), None);
    assert_eq!(total([0.0].into_iter()), Some(0.0));
    assert_eq!(total([2.0, 4.0].into_iter()), Some(6.0));
}

/// The median of the available measurements, or no measurement.
pub(crate) fn median(durations: impl Iterator<Item = f64>) -> Option<f64> {
    let mut durations: Vec<_> = durations.collect();
    durations.sort_by(f64::total_cmp);
    let middle = durations.len() / 2;
    let upper = *durations.get(middle)?;
    Some(if durations.len().is_multiple_of(2) {
        (durations[middle - 1] + upper) * 0.5
    } else {
        upper
    })
}

#[test]
fn a_median_uses_only_available_samples_and_averages_an_even_middle_pair() {
    assert_eq!(
        median([Some(4.0), None, Some(2.0)].into_iter().flatten()),
        Some(3.0)
    );
    assert_eq!(
        median([Some(5.0), Some(1.0), Some(3.0)].into_iter().flatten()),
        Some(3.0)
    );
    assert_eq!(median([Some(0.0), None].into_iter().flatten()), Some(0.0));
    assert_eq!(median([None, None].into_iter().flatten()), None);
}

fn frame(durations: &[Option<f64>]) -> FrameTiming {
    FrameTiming {
        frame_id: 7,
        passes: (0..)
            .zip(durations)
            .map(|(level, duration_ns)| crate::PassTiming {
                pass_id: PassId::Stage(crate::StageId::new("stage", level)),
                duration_ns: *duration_ns,
            })
            .collect(),
    }
}

#[test]
fn a_frame_whose_last_pass_has_no_duration_is_accepted_and_keeps_its_order() {
    let durations = checked_durations("test", &frame(&[Some(1.0), Some(2.0), None]));
    let found: Vec<_> = durations.iter().map(|(_, duration)| *duration).collect();
    assert_eq!(found, [Some(1.0), Some(2.0), None]);
    let levels: Vec<_> = durations
        .iter()
        .map(|(id, _)| match id {
            PassId::Stage(stage) => stage.level,
            other => panic!("unexpected pass {other:?}"),
        })
        .collect();
    assert_eq!(levels, [0, 1, 2]);
}

#[test]
fn a_frame_of_one_untimed_pass_is_accepted() {
    assert_eq!(checked_durations("test", &frame(&[None])).len(), 1);
}

#[test]
#[should_panic(expected = "pass 0 of 3, has no duration and is not the last pass stamped")]
fn a_frame_whose_first_pass_has_no_duration_is_refused() {
    checked_durations("test", &frame(&[None, Some(2.0), Some(3.0)]));
}

#[test]
#[should_panic(expected = "pass 1 of 3, has no duration and is not the last pass stamped")]
fn a_frame_whose_middle_pass_has_no_duration_is_refused_even_when_the_last_has_none() {
    checked_durations("test", &frame(&[Some(1.0), None, None]));
}
