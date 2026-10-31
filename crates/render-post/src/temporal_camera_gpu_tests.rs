//! The camera term of the temporal reconstruction on lavapipe. The analytic image of
//! `temporal_quality_scene` pans across the output by a constant number of canvas pixels per
//! frame with zero object residual, so the whole displacement is camera motion. The camera block
//! carries the reprojection of that pan: for a pan of `d` canvas pixels the content at output
//! position `p` was at `p + d` in the previous frame, which in NDC (+y up) is a translation of
//! `(2 dx / W, -2 dy / H)`. The reference is the box filtered scene shifted by the total pan.
use crate::temporal_quality_scene as scene;
use crate::temporal_quality_tests::{Errors, OUTPUT, Rig, errors, truth_panned};
use crate::{RenderScale, TemporalCamera};

/// Last frames averaged, after two full jitter cycles.
const MEASURED: u64 = 8;

fn translation(pan: [f64; 2], sign: f64) -> TemporalCamera {
    let mut reprojection = [[0.0; 4]; 4];
    for (i, column) in reprojection.iter_mut().enumerate() {
        column[i] = 1.0;
    }
    reprojection[3][0] = sign * 2.0 * pan[0] / scene::CANVAS[0];
    reprojection[3][1] = -sign * 2.0 * pan[1] / scene::CANVAS[1];
    TemporalCamera {
        reprojection,
        depth_terms: [0.0; 2],
        valid: true,
    }
}

/// Mean error over the last `MEASURED` frames of two jitter cycles of a pan of `pan` per frame.
/// `camera` is the sign of the uploaded term: `None` uploads nothing, the stage as it was.
fn pan_error(rig: &Rig, pan: [f64; 2], camera: Option<f64>, accumulate: bool) -> Errors {
    let frames = 2 * u64::from(rig.jitter.length());
    let mut sum = Errors::default();
    for index in 0..frames {
        let total = pan.map(|p| p * f64::from(u32::try_from(index).unwrap()));
        let term = camera.map(|sign| translation(pan, sign));
        rig.frame_panned(index, accumulate && index > 0, (total, term));
        if index >= frames - MEASURED {
            let e = errors(&truth_panned(total), &rig.image());
            sum.all += e.all / f64::from(u32::try_from(MEASURED).unwrap());
            sum.edges += e.edges / f64::from(u32::try_from(MEASURED).unwrap());
        }
    }
    sum
}

struct Row {
    fixed: Errors,
    absent: Errors,
    wrong: Errors,
    reset: Errors,
}

fn measure(render: [u32; 2], pan: [f64; 2]) -> Row {
    let gpu = crate::gpu_tests::gpu();
    let run = |camera, accumulate| pan_error(&Rig::new(&gpu, render), pan, camera, accumulate);
    let row = Row {
        fixed: run(Some(1.0), true),
        absent: run(None, true),
        wrong: run(Some(-1.0), true),
        reset: run(None, false),
    };
    eprintln!(
        "pan {pan:?} R {render:?}: camera {:?} | PR2 {:?} | wrong sign {:?} | reset {:?}",
        row.fixed, row.absent, row.wrong, row.reset
    );
    row
}

/// Margins over the measured ratios on lavapipe (edge band error, four cases): camera term over
/// the spatial floor 0.72 to 0.73, no term over camera term 1.8 to 2.5, wrong sign over camera
/// term 1.23 to 1.39. Each limit leaves more than the 8 to 10% lavapipe to Radeon spread.
///
/// With the term the history is aligned, so it only adds the temporal samples: the error is no
/// more than the spatial-only floor (the history is clipped to the current neighbourhood and
/// weighted by `1 - 1.5 |motion|`). Without it the history lags by the pan accumulated over its
/// lifetime, which on a high contrast edge costs a multiple of the aligned error. The wrong sign
/// moves the history twice the pan away from where it belongs.
const FLOOR_RATIO: f64 = 1.0;
const MISSING_TERM_RATIO: f64 = 1.4;
const WRONG_SIGN_RATIO: f64 = 1.1;

fn assert_aligned(row: &Row) {
    assert!(row.fixed.all.is_finite() && row.absent.all.is_finite());
    assert!(
        row.fixed.edges <= FLOOR_RATIO * row.reset.edges,
        "camera term {:?} above the spatial floor {:?}",
        row.fixed,
        row.reset
    );
    assert!(
        row.absent.edges >= MISSING_TERM_RATIO * row.fixed.edges,
        "without the term {:?} is not measurably worse than {:?}",
        row.absent,
        row.fixed
    );
    assert!(
        row.wrong.edges >= WRONG_SIGN_RATIO * row.fixed.edges,
        "wrong sign {:?} is not measurably worse than {:?}",
        row.wrong,
        row.fixed
    );
}

#[test]
fn a_horizontal_pan_lands_at_the_analytic_position() {
    assert_aligned(&measure([80, 48], [0.25, 0.0]));
}

#[test]
fn a_vertical_pan_lands_at_the_analytic_position() {
    assert_aligned(&measure([80, 48], [0.0, 0.25]));
}

#[test]
fn a_diagonal_pan_lands_at_the_analytic_position() {
    assert_aligned(&measure([80, 48], [0.2, -0.15]));
}

#[test]
fn unequal_extents_scale_the_pan_per_axis() {
    let render = RenderScale::new(670_000).unwrap().extent(OUTPUT).unwrap();
    assert_aligned(&measure([render[0], 56], [0.25, 0.2]));
}

#[test]
fn an_identity_camera_is_the_stage_without_one_byte_for_byte() {
    let gpu = crate::gpu_tests::gpu();
    let identity = TemporalCamera {
        valid: true,
        ..translation([0.0; 2], 1.0)
    };
    let run = |term: Option<TemporalCamera>| {
        let rig = Rig::new(&gpu, [80, 48]);
        let mut images = Vec::new();
        for index in 0..12 {
            rig.frame_panned(index, index > 0, ([0.0; 2], term));
            images.push(rig.image());
        }
        images
    };
    assert_eq!(run(Some(identity)), run(None));
}
