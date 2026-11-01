//! Measurement of the old-colour trail of a translucent mover before reactive masks, on lavapipe.
//!
//! `temporal_translucent_scene` moves a 24 x 48 bar of 70% white over the disocclusion scene's
//! checkerboard and stripe plane, 2 canvas pixels per frame, with the zero motion and background
//! depth a translucent draw leaves. The reference is the same function box filtered over each
//! output pixel with 4x4 supersampling in f64. Two paths run the same frames: temporal
//! reconstruction with accumulated history, and the same stage with the history reset every frame
//! (the spatial reconstruction only, the floor a full history rejection can reach).
//!
//! The trail region is the strip the bar vacated during the last `UNCOVERED_FRAMES` frames, as
//! for the disocclusion measurement, where only the opaque background shows. The static region is
//! the stripe plane, which the bar never touches. Errors are the mean absolute HDR error against
//! the reference, averaged over the last `MEASURED` frames of `FRAMES`.
use crate::taa_reconstruct_test_rig::static_camera;
use crate::temporal_disocclusion_scene as opaque;
use crate::temporal_disocclusion_tests::{
    FRAMES, MEASURED, Pixel, index, mean, plane_pixels, strip_pixels,
};
use crate::temporal_scene_test_rig::SceneRig;
use crate::temporal_translucent_scene as scene;
use worldrunner_render_plan::post::RenderScale;

/// Metrics averaged over the last `MEASURED` frames.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    trail_error: f64,
    static_error: f64,
}

/// Box filtered truth of `pixel` for the bar at `left`, 4x4 samples.
fn truth(pixel: Pixel, left: f64) -> f64 {
    const SUB: u32 = 4;
    let at = |v: u32, sub: u32| f64::from(v) + (f64::from(sub) + 0.5) / f64::from(SUB);
    let mut sum = 0.0;
    for j in 0..SUB {
        for i in 0..SUB {
            sum += scene::value([at(pixel[0], i), at(pixel[1], j)], left);
        }
    }
    sum / f64::from(SUB * SUB)
}

fn error(image: &[f64], pixels: &[Pixel], left: f64) -> f64 {
    mean(
        pixels
            .iter()
            .map(|&p| (image[index(p)] - truth(p, left)).abs()),
    )
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the bar's left edge is a small canvas coordinate"
)]
fn frame(rig: &SceneRig, index: u64, history_valid: bool) {
    let left = opaque::bar_left(index) as f32;
    let tail = [scene::OFF_CANVAS, 0.0, left, 0.0];
    rig.frame(index, history_valid, (tail, Some(static_camera())));
}

fn run(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], accumulate: bool) -> Metrics {
    let rig = SceneRig::new(gpu, render, scene::wgsl());
    let plane = plane_pixels();
    let (mut trail, mut fixed) = (Vec::new(), Vec::new());
    for index in 0..FRAMES {
        frame(&rig, index, accumulate && index > 0);
        if index + MEASURED < FRAMES {
            continue;
        }
        let image = rig.image();
        let left = opaque::bar_left(index);
        trail.push(error(&image, &strip_pixels(index), left));
        fixed.push(error(&image, &plane, left));
    }
    Metrics {
        trail_error: mean(trail.into_iter()),
        static_error: mean(fixed.into_iter()),
    }
}

/// The path with accumulated history and the reset floor on one device and one scene.
fn measure(millionths: u32) -> [Metrics; 2] {
    let gpu = crate::gpu_tests::gpu();
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(crate::temporal_scene_test_rig::OUTPUT)
        .unwrap();
    let accumulated = run(&gpu, render, true);
    let reset = run(&gpu, render, false);
    eprintln!(
        "scale {millionths} R {render:?}: accumulated {accumulated:?} | reset {reset:?} | trail over reset {:.4}",
        accumulated.trail_error / reset.trail_error
    );
    [accumulated, reset]
}

/// The baseline of the path before reactive masks, measured on lavapipe (mean absolute HDR error
/// over the last eight of 48 frames; bar 24 x 48 pixels, 70% white, 2 pixels per frame).
///
/// | scale | R | trail, accumulated | trail, reset | trail ratio | static, accumulated | static, reset |
/// |---|---|---|---|---|---|---|
/// | 0.50 | 80x48 | 0.0798 | 0.0436 | 1.83 | 0.0714 | 0.1080 |
/// | 0.67 | 107x64 | 0.0622 | 0.0324 | 1.92 | 0.0383 | 0.0828 |
/// | 0.77 | 123x74 | 0.0510 | 0.0305 | 1.67 | 0.0336 | 0.0725 |
///
/// The static numbers equal the stripe plane of the disocclusion measurement, as they must: the
/// bar never touches it. The history keeps the old colour of the bar in the vacated strip, so the
/// accumulated error is 1.67 to 1.92 times the reset error, which is the floor a full history
/// rejection reaches (checker edges inside the strip). The assertions are only what holds today:
/// every error is finite and positive and the trail is at least 1.15 times the reset error, a
/// margin above the 8 to 10% spread between devices.
fn assert_baseline([accumulated, reset]: [Metrics; 2]) {
    for m in [accumulated, reset] {
        for v in [m.trail_error, m.static_error] {
            assert!(v.is_finite() && v > 0.0, "{m:?}");
        }
    }
    assert!(
        accumulated.trail_error >= 1.15 * reset.trail_error,
        "{accumulated:?} {reset:?}"
    );
}

#[test]
fn half_scale_translucent_trail() {
    assert_baseline(measure(500_000));
}

#[test]
fn two_thirds_scale_translucent_trail() {
    assert_baseline(measure(670_000));
}

#[test]
fn three_quarters_scale_translucent_trail() {
    assert_baseline(measure(770_000));
}
