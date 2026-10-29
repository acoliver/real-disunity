//! Depth-history disocclusion in the f64 reference: history is rejected where the surface now
//! visible was not the one stored in history.
use crate::TaaSettings;
use crate::cas_reference::Image;
use crate::taa_reference::{Depth, Frame, IDENTITY};
use crate::taa_velocity::Velocity;

const NEAR: f64 = 0.1;
const FAR: f64 = 100.0;
const SIZE: [i32; 2] = [16, 8];

/// Depth terms of a standard-Z perspective projection.
pub(crate) fn terms() -> [f64; 2] {
    [FAR / (FAR - NEAR), -NEAR * FAR / (FAR - NEAR)]
}

/// Device depth of linear view depth `d`.
pub(crate) fn device(d: f64) -> f32 {
    let [a, b] = terms();
    #[expect(clippy::cast_possible_truncation, reason = "the test stores f32 depth")]
    let z = (a + b / d) as f32;
    z
}

fn gray(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// A checkerboard whose range 0.3..0.7 contains the bar colour, so the colour box accepts it.
pub(crate) fn checker(x: i32, y: i32) -> f32 {
    if (x + y) % 2 == 0 { 0.3 } else { 0.7 }
}

const BAR: f32 = 0.5;
const BAR_DEPTH: f64 = 5.0;
const BACKGROUND_DEPTH: f64 = 20.0;

fn colour(bar: std::ops::Range<i32>) -> Image {
    Image::from_fn(SIZE, |x, y| {
        gray(if bar.contains(&x) { BAR } else { checker(x, y) })
    })
}

fn depth(bar: std::ops::Range<i32>) -> Depth {
    Depth::from_fn(SIZE, |x, _| {
        device(if bar.contains(&x) {
            BAR_DEPTH
        } else {
            BACKGROUND_DEPTH
        })
    })
}

fn stored(bar: std::ops::Range<i32>) -> Depth {
    Depth::from_fn(SIZE, |x, _| {
        #[expect(clippy::cast_possible_truncation, reason = "the test stores f32 depth")]
        let d = (if bar.contains(&x) {
            BAR_DEPTH
        } else {
            BACKGROUND_DEPTH
        }) as f32;
        d
    })
}

/// The bar moves from x 4..8 to x 7..11 on a static camera; its pixels carry the residual.
pub(crate) struct Sweep {
    pub(crate) color: Image,
    pub(crate) depth: Depth,
    pub(crate) history: Image,
    pub(crate) stored: Depth,
    pub(crate) velocity: Velocity,
}
impl Sweep {
    pub(crate) fn new() -> Self {
        Self {
            color: colour(7..11),
            depth: depth(7..11),
            history: colour(4..8),
            stored: stored(4..8),
            velocity: Velocity::from_fn(SIZE, 1, |x, _, _| {
                if (7..11).contains(&x) {
                    [-3.0 / 16.0, 0.0]
                } else {
                    [0.0; 2]
                }
            }),
        }
    }

    fn resolve(&self, with_depth: bool, at: [i32; 2]) -> f64 {
        Frame {
            color: &self.color,
            depth: &self.depth,
            history: &self.history,
            reprojection: IDENTITY,
            jitter: [0.0; 2],
            history_valid: true,
            settings: TaaSettings::DEFAULT,
            samples: 1,
            velocity: Some(&self.velocity),
            depth_history: with_depth.then_some(&self.stored),
            depth_terms: if with_depth { terms() } else { [0.0; 2] },
        }
        .resolve_pixel(at)[0]
    }
}

fn single(history_depth: f32) -> (f64, f64) {
    let color = Image::from_fn([5, 5], |x, y| gray(checker(x, y)));
    let depth = Depth::from_fn([5, 5], |_, _| device(BACKGROUND_DEPTH));
    let history = Image::from_fn([5, 5], |_, _| gray(0.5));
    let stored = Depth::from_fn([5, 5], |_, _| history_depth);
    let resolve = |with_depth: bool| {
        Frame {
            color: &color,
            depth: &depth,
            history: &history,
            reprojection: IDENTITY,
            jitter: [0.0; 2],
            history_valid: true,
            settings: TaaSettings::DEFAULT,
            samples: 1,
            velocity: None,
            depth_history: with_depth.then_some(&stored),
            depth_terms: if with_depth { terms() } else { [0.0; 2] },
        }
        .resolve_pixel([2, 2])[0]
    };
    (resolve(true), resolve(false))
}

#[test]
fn history_stored_at_another_depth_is_replaced_by_the_current_colour() {
    // Current colour at (2, 2) is 0.3 and history 0.5 is inside the colour box.
    let (rejected, accepted) = single(5.0);
    assert!((rejected - 0.3).abs() < 1e-6, "{rejected}");
    assert!(
        accepted > 0.45,
        "without the test history dominates: {accepted}"
    );
}

#[test]
fn history_stored_within_the_tolerance_blends_as_before() {
    let (within, without) = single(20.0 * 1.015);
    assert!((within - without).abs() < 1e-12, "{within} {without}");
    let (beyond, _) = single(20.0 * 1.03);
    assert!((beyond - 0.3).abs() < 1e-6, "{beyond}");
}

#[test]
fn sky_over_sky_is_not_rejected() {
    let color = Image::from_fn([5, 5], |x, y| gray(checker(x, y)));
    let depth = Depth::from_fn([5, 5], |_, _| 1.0);
    let history = Image::from_fn([5, 5], |_, _| gray(0.5));
    let [a, b] = terms();
    #[expect(clippy::cast_possible_truncation, reason = "the test stores f32 depth")]
    let far = (b / (1.0 - a)) as f32;
    let stored = Depth::from_fn([5, 5], |_, _| far);
    let mut reprojection = IDENTITY;
    reprojection[3][3] = 1.5;
    let frame = |with_terms: bool| Frame {
        color: &color,
        depth: &depth,
        history: &history,
        reprojection,
        jitter: [0.0; 2],
        history_valid: true,
        settings: TaaSettings::DEFAULT,
        samples: 1,
        velocity: None,
        depth_history: with_terms.then_some(&stored),
        depth_terms: if with_terms { terms() } else { [0.0; 2] },
    };
    let with = frame(true).resolve_pixel([2, 2])[0];
    assert!(
        (with - frame(false).resolve_pixel([2, 2])[0]).abs() < 1e-12,
        "{with}"
    );
}

#[test]
fn the_stored_depth_is_the_linear_depth_of_the_closest_neighbour() {
    let sweep = Sweep::new();
    let frame = Frame {
        color: &sweep.color,
        depth: &sweep.depth,
        history: &sweep.history,
        reprojection: IDENTITY,
        jitter: [0.0; 2],
        history_valid: true,
        settings: TaaSettings::DEFAULT,
        samples: 1,
        velocity: None,
        depth_history: None,
        depth_terms: terms(),
    };
    assert!((frame.stored_depth([12, 3]) - BACKGROUND_DEPTH).abs() < 1e-3);
    assert!((frame.stored_depth([8, 3]) - BAR_DEPTH).abs() < 1e-3);
    // x = 6 has the bar in its 3x3 neighbourhood.
    assert!((frame.stored_depth([6, 3]) - BAR_DEPTH).abs() < 1e-3);
}

#[test]
fn a_bar_sweeping_over_a_similar_background_leaves_less_error_in_the_uncovered_strip() {
    let sweep = Sweep::new();
    let mut error = [0.0, 0.0];
    for y in 1..7 {
        // x = 4 and 5 are bare background now and held the bar in the previous frame.
        for x in [4, 5] {
            let truth = f64::from(checker(x, y));
            for (e, with_depth) in error.iter_mut().zip([true, false]) {
                *e += (sweep.resolve(with_depth, [x, y]) - truth).abs();
            }
        }
    }
    let [with, without] = error.map(|e| e / 12.0);
    // Measured: 0 with the test against 0.18 without it (0.9 of the 0.2 colour difference).
    assert!(with < 1e-9, "{with}");
    assert!(without > 0.1, "{without}");
    assert!(with < without / 10.0);
}

#[test]
fn pixels_with_an_object_residual_are_not_depth_tested() {
    let sweep = Sweep::new();
    // x = 9 is inside the bar: residual -3/16 finds the bar's history at x = 6..., which
    // stored the background, but the residual is XY only so no depth verdict is made.
    let tested = sweep.resolve(true, [9, 3]);
    let untested = sweep.resolve(false, [9, 3]);
    assert!((tested - untested).abs() < 1e-12, "{tested} {untested}");
}
