//! Behavior of the f64 temporal-upscale reference. Every bound is derived in its test.
use crate::TaaSettings;
use crate::cas_reference::Image;
use crate::temporal_upscale_reference::Temporal;

const EPS: f64 = f64::EPSILON;

/// Largest first-moment error of normalised Lanczos2 over the four taps, in internal pixels per
/// unit slope. Lanczos2 sums to one after normalisation but is not linear-exact: the offset
/// `sum(d * L(d)) / sum(L(d))` is zero at fractions 0 and 1/2 and peaks at 0.0317 near 0.227
/// (dense scan of the kernel over fractions 0..1 in steps of 1/1000), so 0.032 bounds it.
const RAMP_PHASE_ERROR: f64 = 0.032;

fn settings(history_weight: f32) -> TaaSettings {
    TaaSettings {
        history_weight,
        ..TaaSettings::default()
    }
}

fn image(size: [i32; 2], mut f: impl FnMut(i32, i32) -> f32) -> Image {
    Image::from_fn(size, |x, y| {
        let v = f(x, y);
        [v, v, v, 0.5]
    })
}

fn frame<'a>(color: &'a Image, history: &'a Image, jitter: [f64; 2], weight: f32) -> Temporal<'a> {
    Temporal {
        color,
        history,
        output: history.size,
        jitter,
        history_valid: true,
        settings: settings(weight),
        motion_uv: [0.0; 2],
    }
}

/// Output pixels whose 4x4 internal footprint lies inside the image.
fn interior(t: &Temporal<'_>) -> Vec<[i32; 2]> {
    let [rw, rh] = t.color.size;
    let mut out = Vec::new();
    for y in 0..t.output[1] {
        for x in 0..t.output[0] {
            let q = t.sample_position([x, y]);
            let (bx, by) = (q[0].floor(), q[1].floor());
            if bx >= 1.0 && bx <= f64::from(rw - 3) && by >= 1.0 && by <= f64::from(rh - 3) {
                out.push([x, y]);
            }
        }
    }
    out
}

#[test]
fn sample_position_maps_output_centres_through_both_extents_and_the_jitter() {
    let color = image([8, 6], |_, _| 0.0);
    let history = image([16, 12], |_, _| 0.0);
    let t = frame(&color, &history, [0.25, -0.125], 0.0);
    // u = (p + 0.5) / O and q = u * R + jitter - 0.5, per axis, worked by hand:
    // x: 0.5 / 16 * 8 + 0.25 - 0.5 = 0 and 15.5 / 16 * 8 + 0.25 - 0.5 = 7.5;
    // y: 0.5 / 12 * 6 - 0.125 - 0.5 = -0.375 and 11.5 / 12 * 6 - 0.125 - 0.5 = 5.125.
    for (at, expected) in [([0, 0], [0.0, -0.375]), ([15, 11], [7.5, 5.125])] {
        let q = t.sample_position(at);
        assert!((q[0] - expected[0]).abs() <= 8.0 * EPS, "{at:?} {q:?}");
        assert!((q[1] - expected[1]).abs() <= 8.0 * EPS, "{at:?} {q:?}");
    }
}

/// Normalised weights make a constant a fixed point. The error is the f64 rounding of at most
/// sixteen products and fifteen additions plus one division, each within `EPS` relative to values
/// not above `1.3 * c` (the weight sum is 1.01 at worst and the kernel has small negative lobes),
/// so `64 * EPS * c` bounds it with margin.
#[test]
fn a_constant_field_is_preserved_exactly_up_to_f64_rounding() {
    for (render, output) in [([7, 5], [11, 8]), ([9, 9], [9, 9]), ([6, 4], [12, 8])] {
        for jitter in [[0.0, 0.0], [0.3, -0.4], [-0.45, 0.45]] {
            let c = 0.37;
            let color = image(render, |_, _| c);
            let history = image(output, |_, _| c);
            let t = frame(&color, &history, jitter, 0.0);
            for y in 0..output[1] {
                for x in 0..output[0] {
                    let out = t.reconstruct([x, y]);
                    assert!(
                        (out[0] - f64::from(c)).abs() <= 64.0 * EPS * f64::from(c),
                        "{render:?}->{output:?} {jitter:?} at {x},{y}: {out:?}"
                    );
                    assert!((out[3] - 0.5).abs() <= 64.0 * EPS * 0.5);
                }
            }
        }
    }
}

/// With the jitter chosen so `q` is an integer or a half integer the Lanczos2 fraction is 0 or
/// 1/2, where the first-moment error is zero, so an affine ramp is reproduced to f64 rounding.
/// The ramp values are dyadic and exact in f32; the sums are at most 1.0 and the kernel
/// magnitudes below 1.3, so `64 * EPS` bounds the rounding.
#[test]
fn an_affine_ramp_is_reproduced_at_exact_phases() {
    let ramp = |x: f64, y: f64| 0.5 + x / 16.0 + y / 32.0;
    let color = image([8, 8], |x, y| ramp(f64::from(x), f64::from(y)) as f32);
    let history = image([16, 16], |_, _| 0.0);
    // q = p / 2 - 0.25 + jitter, so jitter 0.25 puts q on integers and half integers.
    let t = frame(&color, &history, [0.25, 0.25], 0.0);
    let pixels = interior(&t);
    assert!(pixels.len() > 40);
    for at in pixels {
        let q = t.sample_position(at);
        let out = t.reconstruct(at);
        assert!(
            (out[0] - ramp(q[0], q[1])).abs() <= 64.0 * EPS,
            "{at:?} q {q:?}: {out:?}"
        );
    }
}

/// Between those phases the offset is at most `RAMP_PHASE_ERROR` pixels per axis, so the error is
/// that many pixels times the slope on each axis, plus f64 rounding.
#[test]
fn an_affine_ramp_is_reproduced_within_the_kernel_phase_error_everywhere() {
    let (sx, sy) = (1.0 / 16.0, 1.0 / 32.0);
    let ramp = |x: f64, y: f64| 0.5 + sx * x + sy * y;
    let color = image([10, 9], |x, y| ramp(f64::from(x), f64::from(y)) as f32);
    let history = image([17, 13], |_, _| 0.0);
    for jitter in [[0.0, 0.0], [0.3, -0.2], [-0.41, 0.43]] {
        let t = frame(&color, &history, jitter, 0.0);
        assert!(interior(&t).len() > 20);
        for at in interior(&t) {
            let q = t.sample_position(at);
            let out = t.reconstruct(at);
            let bound = RAMP_PHASE_ERROR * (sx + sy) + 64.0 * EPS;
            assert!(
                (out[0] - ramp(q[0], q[1])).abs() <= bound,
                "{at:?} {jitter:?}"
            );
        }
    }
}

/// A scene whose value is its world coordinate `u` is stored at internal extent `R` as
/// `f(s) = (s + 0.5 - jitter) / R`. The reconstruction at output pixel `p` must return
/// `(p + 0.5) / O` for every `R`, within the kernel phase error `0.032 / R` and rounding. The
/// opposite jitter sign would be off by `2 * jitter / R`, which the last assertion shows exceeds
/// the bound for each case, so this test fails for a wrong sign.
#[test]
fn the_jitter_sign_places_a_world_ramp_at_the_same_output_position_for_any_internal_extent() {
    let output = [16, 16];
    let history = image(output, |_, _| 0.0);
    for render in [8, 10, 12, 16] {
        for jitter in [[0.3, -0.3], [-0.4, 0.2]] {
            let r = f64::from(render);
            let color = image([render, render], |x, y| {
                let (sx, sy) = (f64::from(x), f64::from(y));
                (((sx + 0.5 - jitter[0]) / r) + ((sy + 0.5 - jitter[1]) / r)) as f32
            });
            let t = frame(&color, &history, jitter, 0.0);
            let bound = 2.0 * RAMP_PHASE_ERROR / r + 1e-6;
            let wrong = 2.0 * jitter[0].abs().min(jitter[1].abs()) / r;
            assert!(
                wrong > bound,
                "R {render} jitter {jitter:?} cannot expose a sign error"
            );
            let pixels = interior(&t);
            assert!(!pixels.is_empty());
            for at in pixels {
                let u = (f64::from(at[0]) + 0.5) / 16.0 + (f64::from(at[1]) + 0.5) / 16.0;
                let out = t.reconstruct(at);
                assert!(
                    (out[0] - u).abs() <= bound,
                    "R {render} {jitter:?} {at:?}: {out:?} vs {u}"
                );
            }
        }
    }
}

#[test]
fn a_zero_history_weight_gives_the_spatial_reconstruction_whatever_the_history_holds() {
    let color = image([8, 8], |x, y| {
        0.2 + 0.05 * f32::from(u8::try_from((x * 3 + y) % 5).unwrap())
    });
    let history = image([14, 12], |x, y| {
        3.0 + f32::from(u8::try_from((x + y) % 4).unwrap())
    });
    let t = frame(&color, &history, [0.2, -0.3], 0.0);
    for y in 0..12 {
        for x in 0..14 {
            let (a, b) = (t.resolve_pixel([x, y]), t.reconstruct([x, y]));
            for c in 0..3 {
                assert!(
                    (a[c] - b[c]).abs() <= 64.0 * EPS * 4.0,
                    "{x},{y} {a:?} {b:?}"
                );
            }
        }
    }
}

#[test]
fn invalid_history_returns_the_reconstructed_current() {
    let color = image([8, 8], |x, _| 0.1 * f32::from(u8::try_from(x).unwrap()));
    let history = image([12, 12], |_, _| 9.0);
    let mut t = frame(&color, &history, [0.1, 0.1], 0.9);
    t.history_valid = false;
    for at in [[0, 0], [5, 6], [11, 11]] {
        // The resolve is nonnegative; the Lanczos lobes may undershoot below zero.
        let current = t.reconstruct(at).map(|v| v.max(0.0));
        assert_eq!(t.resolve_pixel(at)[..3], current[..3]);
    }
}

#[test]
fn history_far_outside_the_neighbourhood_is_clipped_to_the_current_colour() {
    let color = image([8, 8], |_, _| 0.2);
    let history = image([12, 12], |_, _| 50.0);
    let t = frame(&color, &history, [0.0, 0.0], 0.9);
    // The box of a constant neighbourhood has its minimum extent, so clipped history equals
    // the current colour to within that extent, not the raw 50.
    let out = t.resolve_pixel([6, 6]);
    assert!((out[0] - 0.2).abs() < 1e-3, "{out:?}");
}

#[test]
fn matching_history_in_a_static_scene_is_a_fixed_point() {
    let color = image([8, 8], |_, _| 0.6);
    let history = image([12, 12], |_, _| 0.6);
    let t = frame(&color, &history, [0.3, -0.1], 0.9);
    let out = t.resolve_pixel([4, 5]);
    assert!((out[0] - f64::from(0.6_f32)).abs() <= 1e-9, "{out:?}");
}

#[test]
fn history_outside_the_image_is_ignored() {
    let color = image([8, 8], |_, _| 0.2);
    let history = image([12, 12], |_, _| 0.9);
    let mut t = frame(&color, &history, [0.0, 0.0], 0.9);
    t.motion_uv = [2.0, 0.0];
    let out = t.resolve_pixel([4, 4]);
    assert!((out[0] - f64::from(0.2_f32)).abs() <= 64.0 * EPS);
}

#[test]
fn non_finite_and_negative_current_samples_never_reach_the_output() {
    let mut bad = [f32::NAN, f32::INFINITY, -2.0, 0.5].into_iter().cycle();
    let color = image([6, 6], |_, _| bad.next().unwrap());
    let history = image([9, 9], |_, _| 0.5);
    let t = frame(&color, &history, [0.1, 0.2], 0.9);
    for y in 0..9 {
        for x in 0..9 {
            let out = t.resolve_pixel([x, y]);
            assert!(
                out.iter().all(|v| v.is_finite() && *v >= 0.0),
                "{x},{y} {out:?}"
            );
        }
    }
}
