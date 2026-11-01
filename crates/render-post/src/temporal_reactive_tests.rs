//! Reactive consumption of the prepared mask on the translucent mover, on lavapipe. The rig paints
//! the bar's footprint into the frame mask with a reactive strength; the temporal upscale rejects
//! history in proportion. Every figure is the mean absolute HDR error against the box filtered
//! truth over the last `MEASURED` of `FRAMES`, as in `temporal_translucent_tests`.
use crate::taa_mask_gpu::WithoutMasks;
use crate::temporal_disocclusion_scene as opaque;
use crate::temporal_disocclusion_tests::{
    FRAMES, MEASURED, index, mean, plane_pixels, region, strip_pixels,
};
use crate::temporal_scene_test_rig::SceneRig;
use crate::temporal_translucent_tests::{error, frame};
use worldrunner_render_plan::post::RenderScale;

/// Half of a half float step near 1.
const F16_STEP: f64 = 1.0 / 2048.0;
const STRENGTHS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

#[derive(Clone, Copy, Debug)]
struct Setup {
    /// Reactive strength painted over the bar's footprint.
    reactive: f32,
    /// False resets the history every frame: the reconstruction alone.
    accumulate: bool,
    /// False builds a chain without a mask.
    masked: bool,
}

const ZERO: Setup = Setup {
    reactive: 0.0,
    accumulate: true,
    masked: true,
};

struct Outcome {
    trail: f64,
    fixed: f64,
    /// The last frame.
    last: Vec<f64>,
    /// The bits of the last `MEASURED` frames.
    bits: Vec<u64>,
}

fn run(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], setup: Setup) -> Outcome {
    let _off = (!setup.masked).then(WithoutMasks::new);
    let rig = SceneRig::new(gpu, render, crate::temporal_translucent_scene::wgsl());
    let plane = plane_pixels();
    let (mut trail, mut fixed, mut bits) = (Vec::new(), Vec::new(), Vec::new());
    let mut last = Vec::new();
    for at in 0..FRAMES {
        frame(&rig, at, setup.accumulate && at > 0, setup.reactive);
        if at + MEASURED < FRAMES {
            continue;
        }
        last = rig.image();
        bits.extend(last.iter().map(|v| v.to_bits()));
        let left = opaque::bar_left(at);
        trail.push(error(&last, &strip_pixels(at), left));
        fixed.push(error(&last, &plane, left));
    }
    Outcome {
        trail: mean(trail.into_iter()),
        fixed: mean(fixed.into_iter()),
        last,
        bits,
    }
}

fn extent(millionths: u32) -> [u32; 2] {
    RenderScale::new(millionths)
        .unwrap()
        .extent(crate::temporal_scene_test_rig::OUTPUT)
        .unwrap()
}

fn with(reactive: f32) -> Setup {
    Setup { reactive, ..ZERO }
}

/// Q1 on one scale: the reactive strength only lowers the trail, a painted mover leaves the static
/// plane alone, and a strength of 0.9 over the footprint removes at least half of the trail's
/// excess over the reset floor.
fn assert_q1(millionths: u32) {
    let gpu = crate::gpu_tests::gpu();
    let render = extent(millionths);
    let runs: Vec<_> = STRENGTHS
        .iter()
        .map(|&r| run(&gpu, render, with(r)))
        .collect();
    let reset = run(
        &gpu,
        render,
        Setup {
            accumulate: false,
            ..ZERO
        },
    );
    let producer = run(&gpu, render, with(0.9));
    eprintln!(
        "Q1 scale {millionths} R {render:?}: trail by reactive {:?} | reset {:.5} | 0.9 {:.5} | static zero {:.5} by reactive {:?} | static reset {:.5}",
        runs.iter().map(|o| o.trail).collect::<Vec<_>>(),
        reset.trail,
        producer.trail,
        runs[0].fixed,
        runs.iter().map(|o| o.fixed).collect::<Vec<_>>(),
        reset.fixed,
    );
    for pair in runs.windows(2) {
        assert!(pair[1].trail <= pair[0].trail + F16_STEP, "monotone");
    }
    for outcome in runs.iter().chain([&producer]) {
        assert!(outcome.fixed <= 1.10 * runs[0].fixed, "static ROI");
    }
    let zero = runs[0].trail;
    assert!(
        producer.trail <= reset.trail + 0.5 * (zero - reset.trail),
        "producer form: {} against zero {zero} and reset {}",
        producer.trail,
        reset.trail
    );
}

#[test]
fn half_scale_reactive_trail() {
    assert_q1(500_000);
}

#[test]
fn two_thirds_scale_reactive_trail() {
    assert_q1(670_000);
}

#[test]
fn three_quarters_scale_reactive_trail() {
    assert_q1(770_000);
}

/// Where the dilated mask is 1 the history weight is 0 and the output is the reconstruction of
/// the current frame, which is what the reset path writes everywhere.
#[test]
fn full_reactivity_equals_the_reset_image_inside_the_footprint() {
    let gpu = crate::gpu_tests::gpu();
    let render = extent(500_000);
    let full = run(&gpu, render, with(1.0));
    let reset = run(
        &gpu,
        render,
        Setup {
            accumulate: false,
            ..ZERO
        },
    );
    let left = opaque::bar_left(FRAMES - 1);
    let top = opaque::BAR_TOP;
    let inside = region(
        left + 4.0..left + opaque::BAR_SIZE[0] - 4.0,
        top + 4.0..top + opaque::BAR_SIZE[1] - 4.0,
    );
    assert!(!inside.is_empty());
    for pixel in inside {
        let i = index(pixel);
        assert!(
            (full.last[i] - reset.last[i]).abs() <= F16_STEP,
            "{pixel:?}: {} against {}",
            full.last[i],
            reset.last[i]
        );
    }
}

/// A chain without a mask and one whose mask is all zero write the same bits on one device, and
/// both keep the error of the path before masks (recorded in `temporal_translucent_tests`).
#[test]
fn absent_and_all_zero_masks_are_the_path_before_masks() {
    let gpu = crate::gpu_tests::gpu();
    let render = extent(500_000);
    let absent = run(
        &gpu,
        render,
        Setup {
            masked: false,
            ..ZERO
        },
    );
    let zero = run(&gpu, render, ZERO);
    assert_eq!(absent.bits, zero.bits);
    assert!(
        (zero.trail - 0.07979).abs() <= 0.03 * 0.07979,
        "{}",
        zero.trail
    );
    assert!(
        (zero.fixed - 0.07141).abs() <= 0.03 * 0.07141,
        "{}",
        zero.fixed
    );
}
