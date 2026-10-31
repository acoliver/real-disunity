//! Thin-feature measurement of the temporal reconstruction before any luminance lock exists, on
//! lavapipe. The scenes are the subpixel wires and the masked line grid of
//! `temporal_thin_feature_scene`, rasterised at the internal extent with the jitter of
//! `ProjectionJitter::for_extents`, static and panning along +x at 0.1 and 0.3 canvas pixels per
//! frame (the backward motion is the residual, the camera term is absent).
//!
//! The region of interest is the set of output pixels whose centre is within `ROI_BAND` pixels of a
//! line centre, at least `BORDER` pixels from the frame border. The error is the mean absolute
//! HDR error (mean of the colour channels) against the 8 x 8 box filtered f64 scene of the same
//! frame, averaged over the last `MEASURED` frames of two full jitter cycles. "Reset" is the same
//! pipeline with the history discarded before each frame, the floor a lock must beat.
//!
//! These tests assert only what holds without locks (finite numbers); the numbers they print are
//! the PR 4 baseline for the lock work.
//!
//! Measured on lavapipe (160 x 96 canvas, scales 0.5, 0.67 and 0.77): the temporal ROI error is
//! 0.12 to 0.26 and the reset floor 0.23 to 0.45, so the ratio of the two is 0.43 to 0.78 (worst
//! cases: 0.775 for the grid at 0.5 and 0.3 px per frame, 0.757 for wire 0 at 0.5 and 0.3 px per
//! frame). Error rises with pan speed. A vanished line is back within 2/255 display levels after
//! one frame in every measured case (the neighbourhood clip is the likely cause, not isolated);
//! the residual error after 64 frames is 0.12 to 0.26 levels.
use crate::temporal_scene_test_rig::{OUTPUT, SceneRig};
use crate::temporal_thin_feature_scene::{Fixture, ROI_BAND, fixtures};
use crate::{ProjectionJitter, RenderScale};

const BORDER: u32 = 4;
const MEASURED: u64 = 8;
const SUB: u32 = 8;
const SPEEDS: [f64; 3] = [0.0, 0.1, 0.3];

/// Pixels of the region of interest with the box filtered truth of one frame.
struct Roi {
    index: Vec<usize>,
    truth: Vec<f64>,
}

fn roi(fixture: &Fixture, pan: f64, gain: f64) -> Roi {
    let width = usize::try_from(OUTPUT[0]).unwrap();
    let (mut index, mut truth) = (Vec::new(), Vec::new());
    for y in BORDER..OUTPUT[1] - BORDER {
        for x in BORDER..OUTPUT[0] - BORDER {
            let centre = [f64::from(x) + 0.5 + pan, f64::from(y) + 0.5];
            if fixture.line_distance(centre) > ROI_BAND {
                continue;
            }
            let at =
                |pixel: u32, sub: u32| f64::from(pixel) + (f64::from(sub) + 0.5) / f64::from(SUB);
            let mut sum = 0.0;
            for j in 0..SUB {
                for i in 0..SUB {
                    sum += fixture.value([at(x, i) + pan, at(y, j)], gain);
                }
            }
            index.push(usize::try_from(y).unwrap() * width + usize::try_from(x).unwrap());
            truth.push(sum / f64::from(SUB * SUB));
        }
    }
    Roi { index, truth }
}

fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap())
}

impl Roi {
    /// Mean absolute error of `image`, after mapping both through `map`.
    fn error(&self, image: &[f64], map: fn(f64) -> f64) -> f64 {
        let sum: f64 = self
            .index
            .iter()
            .zip(&self.truth)
            .map(|(&i, &t)| (map(image[i]) - map(t)).abs())
            .sum();
        sum / count(self.index.len())
    }
}

fn hdr(v: f64) -> f64 {
    v
}

/// Frame `index` of a pan at `speed` canvas pixels per frame with the lines at `gain`.
fn frame(rig: &SceneRig, index: u64, valid: bool, speed: f64, gain: f32) {
    let pan = speed * f64::from(u32::try_from(index).unwrap());
    #[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
    let tail = [pan as f32, speed as f32, gain, 0.0];
    rig.frame(index, valid, (tail, None));
}

fn pan_of(speed: f64, index: u64) -> f64 {
    speed * f64::from(u32::try_from(index).unwrap())
}

/// Mean ROI error over the last `MEASURED` frames of two cycles, accumulating or reset each frame.
fn measure(rig: &SceneRig, fixture: &Fixture, speed: f64, accumulate: bool) -> f64 {
    let total = 2 * u64::from(rig.jitter.length());
    let first = total - MEASURED;
    let start = if accumulate { 0 } else { first };
    let mut sum = 0.0;
    for index in start..total {
        frame(rig, index, accumulate && index > 0, speed, 1.0);
        if index >= first {
            sum += roi(fixture, pan_of(speed, index), 1.0).error(&rig.image(), hdr);
        }
    }
    sum / f64::from(u32::try_from(MEASURED).unwrap())
}

fn rig_at(gpu: &(wgpu::Device, wgpu::Queue), millionths: u32, fixture: &Fixture) -> SceneRig {
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(OUTPUT)
        .unwrap();
    assert!(ProjectionJitter::for_extents(render, OUTPUT).is_ok());
    SceneRig::new(gpu, render, fixture.wgsl())
}

/// The bound of a lock-on ROI error relative to the lock-off one on the same device, fixed by the
/// coordinator before any lock code: at most 0.90 on the designated fixtures (the grid, wire 45
/// and wire 90, static and at 0.3 px per frame), at most 1.10 on every other fixture and speed.
/// Both numbers exceed the 8 to 10% lavapipe to Radeon spread of a ratio of two same-device runs.
fn lock_ratio_bound(name: &str, speed: f64) -> f64 {
    let designated = matches!(name, "grid" | "wire 45" | "wire 90")
        && (speed.abs() < 1e-9 || (speed - 0.3).abs() < 1e-9);
    if designated { 0.90 } else { 1.10 }
}

/// Whether the lock-on error `on` meets the bound against the lock-off error `off`.
fn lock_rule_holds(name: &str, speed: f64, on: f64, off: f64) -> bool {
    on <= lock_ratio_bound(name, speed) * off
}

fn sweep(millionths: u32) {
    let gpu = crate::gpu_tests::gpu();
    for fixture in fixtures() {
        let rig = rig_at(&gpu, millionths, &fixture);
        let unlocked = {
            let _off = crate::taa_upscale_gpu::WithoutLocks::new();
            rig_at(&gpu, millionths, &fixture)
        };
        for speed in SPEEDS {
            let temporal = measure(&rig, &fixture, speed, true);
            let reset = measure(&rig, &fixture, speed, false);
            let off = measure(&unlocked, &fixture, speed, true);
            eprintln!(
                "thin scale {millionths} {} speed {speed}: temporal {temporal:.5} reset {reset:.5} ratio {:.3} locks off {off:.5} on/off {:.3}",
                fixture.name,
                temporal / reset,
                temporal / off
            );
            assert!(temporal.is_finite() && reset.is_finite());
            assert!(temporal > 0.0 && reset > 0.0);
            // Without locks in the shader the two pipelines are the same code on one device, so
            // the numbers agree to the bit. This becomes the comparison of `lock_rule_holds`.
            assert_eq!(temporal.to_bits(), off.to_bits());
        }
    }
}

/// 6 designated cases per scale (3 fixtures at 2 speeds), 18 over the three scales.
#[test]
fn lock_bound_designates_eighteen_cases() {
    let designated = |name: &str, speed| lock_ratio_bound(name, speed) < 1.0;
    let per_scale = fixtures()
        .iter()
        .flat_map(|f| SPEEDS.map(|s| (f.name, s)))
        .filter(|&(name, speed)| designated(name, speed))
        .count();
    assert_eq!(per_scale * 3, 18);
    assert!(designated("grid", 0.3) && designated("wire 90", 0.0));
    assert!(!designated("wire 0", 0.3) && !designated("wire 30", 0.0));
    assert!(!designated("grid", 0.1) && !designated("wire 45", 0.1));
}

#[test]
fn lock_rule_margins() {
    assert!(lock_rule_holds("grid", 0.3, 0.90, 1.0));
    assert!(!lock_rule_holds("grid", 0.3, 0.91, 1.0));
    assert!(!lock_rule_holds("wire 45", 0.0, 1.0, 1.0));
    assert!(lock_rule_holds("wire 0", 0.1, 1.10, 1.0));
    assert!(!lock_rule_holds("wire 0", 0.1, 1.11, 1.0));
}

#[test]
fn thin_features_half_scale() {
    sweep(500_000);
}

#[test]
fn thin_features_two_thirds_scale() {
    sweep(670_000);
}

#[test]
fn thin_features_three_quarters_scale() {
    sweep(770_000);
}

/// sRGB encoding of a clamped HDR value on a 0 to 255 scale, the display domain of the bound.
fn display(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

/// Frames after a line vanishes until the ROI is within `limit` display levels of the line-free
/// truth, or `None` within `CAP` frames.
fn frames_to_clear(rig: &SceneRig, fixture: &Fixture, limit: f64) -> (Option<u64>, f64, f64) {
    const CAP: u64 = 64;
    let warm = 2 * u64::from(rig.jitter.length());
    for index in 0..warm {
        frame(rig, index, index > 0, 0.0, 1.0);
    }
    let truth = roi(fixture, 0.0, 0.0);
    let (mut first, mut cleared) = (f64::NAN, None);
    let mut last = f64::NAN;
    for k in 1..=CAP {
        frame(rig, warm + k - 1, true, 0.0, 0.0);
        last = truth.error(&rig.image(), display);
        if k == 1 {
            first = last;
        }
        if cleared.is_none() && last <= limit {
            cleared = Some(k);
        }
    }
    (cleared, first, last)
}

/// Staleness of a vanished line without locks. Recorded on lavapipe (see `target/up5.md`): the
/// frame count and the display-domain ROI error (0 to 255 levels) after the first and the last
/// frame of the window. The bound of the later lock work is two submitted frames at 2/255.
#[test]
fn vanished_line_staleness() {
    let gpu = crate::gpu_tests::gpu();
    for millionths in [500_000, 670_000, 770_000] {
        for fixture in fixtures()
            .into_iter()
            .filter(|f| f.name == "wire 30" || f.name == "grid")
        {
            let rig = rig_at(&gpu, millionths, &fixture);
            let (cleared, first, last) = frames_to_clear(&rig, &fixture, 2.0);
            eprintln!(
                "stale scale {millionths} {}: within 2/255 after {cleared:?} frames, error after 1 frame {first:.3}, after 64 frames {last:.3}",
                fixture.name
            );
            assert!(first.is_finite() && last.is_finite());
        }
    }
}

/// Radiance scales of the normaliser tests as powers of two: 2^-10 (about 1e-3), 1, 2^10 (about
/// 1e3) and 2^13. The scene is stored in `Rgba16Float`, whose largest finite value is 65504, and the
/// brightest texel is 2.7 (gradient plus a line), so 2^13 (22,000) is the largest power of two
/// that is not clipped; the design's 1e6 does not fit the internal format.
const SCALES_EV: [f64; 4] = [-10.0, 0.0, 10.0, 13.0];

/// ROI error divided by the radiance scale, and whether every output texel of the measured
/// frames is finite and nonnegative, for a pan at `speed` with the scene scaled by `2^ev`.
fn scaled_error(rig: &SceneRig, fixture: &Fixture, speed: f64, ev: f64) -> (f64, bool) {
    let scale = ev.exp2();
    let total = 2 * u64::from(rig.jitter.length());
    let first = total - MEASURED;
    let (mut sum, mut sane) = (0.0, true);
    for index in 0..total {
        let pan = pan_of(speed, index);
        #[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
        let tail = [pan as f32, speed as f32, 1.0, ev as f32];
        rig.frame(index, index > 0, (tail, None));
        if index >= first {
            let image = rig.image();
            sane &= image.iter().all(|v| v.is_finite() && *v >= 0.0);
            let relative: Vec<f64> = image.iter().map(|v| v / scale).collect();
            sum += roi(fixture, pan, 1.0).error(&relative, hdr);
        }
    }
    (sum / f64::from(u32::try_from(MEASURED).unwrap()), sane)
}

/// The relative ROI error at every scale stays within 1.10 times the unscaled one and every
/// output texel is finite and nonnegative. 1.10 is the coordinator's no-regression margin, above
/// the 8 to 10% lavapipe to Radeon spread; on lavapipe the measured ratios are 0.97 to 1.02, so
/// the path already holds this before a luminance normaliser exists, and the normaliser must
/// keep it. The error is the temporal path's own, so the bound is one sided: a lower error passes.
#[test]
fn radiance_scale_keeps_outputs_sane() {
    let gpu = crate::gpu_tests::gpu();
    for fixture in fixtures()
        .into_iter()
        .filter(|f| f.name == "wire 45" || f.name == "grid")
    {
        let rig = rig_at(&gpu, 670_000, &fixture);
        let (unscaled, _) = scaled_error(&rig, &fixture, 0.1, 0.0);
        assert!(unscaled > 0.0);
        for ev in SCALES_EV {
            let (error, sane) = scaled_error(&rig, &fixture, 0.1, ev);
            eprintln!(
                "radiance {} ev {ev}: relative ROI error {error:.5} sane {sane}",
                fixture.name
            );
            assert!(sane && error.is_finite());
            assert!(
                error <= 1.10 * unscaled,
                "ev {ev}: {error} against {unscaled}"
            );
        }
    }
}

/// Lock-on image of the frame after `warm` warm-up frames of a 0.1 px per frame pan, then either
/// the next valid frame (`reset` false) or a reset frame (`reset` true), then one more valid frame.
/// Returns the images of the frame at `warm` and of the frame at `warm + 1`.
fn after_warmup(
    gpu: &(wgpu::Device, wgpu::Queue),
    fixture: &Fixture,
    reset: bool,
) -> [Vec<f64>; 2] {
    let _on = crate::taa_upscale_gpu::WithLocks::new();
    let rig = rig_at(gpu, 670_000, fixture);
    let warm = 2 * u64::from(rig.jitter.length());
    for index in 0..warm {
        frame(&rig, index, index > 0, 0.1, 1.0);
    }
    frame(&rig, warm, !reset, 0.1, 1.0);
    let first = rig.image();
    frame(&rig, warm + 1, true, 0.1, 1.0);
    [first, rig.image()]
}

/// A reset discards the lock bank and the reference luma it carries: the first frame after the
/// reset equals the first frame of a fresh chain, and so does the next one, which reads the lock
/// state the reset frame wrote. Byte identity of two runs of the same code on one device. The
/// comparison is not vacuous: without the reset the same frames differ from the fresh chain.
#[test]
fn a_reset_clears_the_lock_and_reference_luma_state() {
    let gpu = crate::gpu_tests::gpu();
    for fixture in fixtures()
        .into_iter()
        .filter(|f| f.name == "wire 45" || f.name == "grid")
    {
        let continued = after_warmup(&gpu, &fixture, false);
        let reset = after_warmup(&gpu, &fixture, true);
        let _on = crate::taa_upscale_gpu::WithLocks::new();
        let fresh_rig = rig_at(&gpu, 670_000, &fixture);
        let warm = 2 * u64::from(fresh_rig.jitter.length());
        frame(&fresh_rig, warm, false, 0.1, 1.0);
        let first = fresh_rig.image();
        frame(&fresh_rig, warm + 1, true, 0.1, 1.0);
        let second = fresh_rig.image();
        assert_eq!(reset[0], first, "{}: reset frame", fixture.name);
        assert_eq!(reset[1], second, "{}: frame after the reset", fixture.name);
        assert_ne!(continued[0], first, "{}: history matters", fixture.name);
        assert_ne!(continued[1], second, "{}: lock state matters", fixture.name);
    }
}

/// The locks change the image of a warmed chain: the lock-on and lock-off pipelines differ after
/// the same frames on the same device, so the lock path is live, not a copy of the PR 4 path.
#[test]
fn locks_change_the_accumulated_image_of_a_thin_feature() {
    let gpu = crate::gpu_tests::gpu();
    let fixture = fixtures()
        .into_iter()
        .find(|f| f.name == "wire 45")
        .unwrap();
    let locked = after_warmup(&gpu, &fixture, false);
    let unlocked = {
        let _off = crate::taa_upscale_gpu::WithoutLocks::new();
        let rig = rig_at(&gpu, 670_000, &fixture);
        let warm = 2 * u64::from(rig.jitter.length());
        for index in 0..=warm {
            frame(&rig, index, index > 0, 0.1, 1.0);
        }
        rig.image()
    };
    assert_ne!(locked[0], unlocked);
}
