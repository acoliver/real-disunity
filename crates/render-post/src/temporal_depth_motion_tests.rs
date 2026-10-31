//! A surface that moves along the view axis, on lavapipe. A square bar of constant colour grows
//! while it approaches and shrinks while it recedes (`temporal_depth_motion_scene`), its device
//! depth falling as it grows, with its analytic object residual in the velocity target and a
//! static camera with depth terms. The depth of the previous frame is not available to the
//! stage: it reconstructs the previous depth by scattering the current one, which equals the
//! true previous depth only for motion in the screen plane.
//!
//! Three paths run the same frames on one device: the new path (dilation and depth clip), the
//! PR 2 path (`WithoutDilation` and `WithoutClip`) and history reset every frame. Two windows of
//! the last `MEASURED` frames are measured: the approach (frames up to the end of the approach)
//! and the recession (the last frames of the sequence). In each window:
//!
//! * the swept ring: pixels between the half size now and the half size `MEASURED` frames ago, a
//!   pixel in from both: newly covered while approaching, uncovered while receding;
//! * the edge band: pixel centres within two pixels of the bar's edge, where the mover is.
//!
//! Errors are mean absolute HDR error against the 4x4 box filtered f64 reference.
use crate::RenderScale;
use crate::taa_reconstruct_test_rig::static_camera;
use crate::temporal_depth_motion_scene as scene;
use crate::temporal_scene_test_rig::{OUTPUT, SceneRig};

const FRAMES: u64 = 2 * scene::APPROACH_FRAMES;
const MEASURED: u64 = 8;
/// The last frame of each window.
const APPROACH_END: u64 = scene::APPROACH_FRAMES;
const RECESSION_END: u64 = FRAMES - 1;

type Pixel = [u32; 2];

fn centre(pixel: Pixel) -> [f64; 2] {
    pixel.map(|v| f64::from(v) + 0.5)
}

fn pixels(keep: impl Fn(f64) -> bool) -> Vec<Pixel> {
    let mut found = Vec::new();
    for py in 0..OUTPUT[1] {
        for px in 0..OUTPUT[0] {
            if keep(scene::distance(centre([px, py]))) {
                found.push([px, py]);
            }
        }
    }
    found
}

/// The pixels swept between the half sizes of `frame - MEASURED` and `frame`, a pixel in.
fn ring(frame: u64) -> Vec<Pixel> {
    let (a, b) = (scene::half(frame - MEASURED), scene::half(frame));
    let (low, high) = (a.min(b) + 1.0, a.max(b) - 1.0);
    pixels(|d| d > low && d < high)
}

fn edge_band(frame: u64) -> Vec<Pixel> {
    let edge = scene::half(frame);
    pixels(|d| (d - edge).abs() <= 2.0)
}

fn truth(pixel: Pixel, half: f64) -> f64 {
    const SUB: u32 = 4;
    let at = |v: u32, sub: u32| f64::from(v) + (f64::from(sub) + 0.5) / f64::from(SUB);
    let mut sum = 0.0;
    for j in 0..SUB {
        for i in 0..SUB {
            sum += scene::value([at(pixel[0], i), at(pixel[1], j)], half);
        }
    }
    sum / f64::from(SUB * SUB)
}

fn error(image: &[f64], region: &[Pixel], half: f64) -> f64 {
    let sum: f64 = region
        .iter()
        .map(|&p| (image[usize::try_from(p[1] * OUTPUT[0] + p[0]).unwrap()] - truth(p, half)).abs())
        .sum();
    sum / f64::from(u32::try_from(region.len()).unwrap())
}

/// Ring and edge errors of one window, averaged over its frames.
#[derive(Clone, Copy, Debug)]
struct Window {
    ring: f64,
    edge: f64,
}

#[derive(Clone, Copy, Debug)]
struct Measured {
    approach: Window,
    recession: Window,
}

#[derive(Clone, Copy)]
enum Path {
    New,
    Pr2,
    Reset,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "half sizes, rates and depths are small canvas values"
)]
fn run(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], path: Path) -> Measured {
    let _undilated =
        matches!(path, Path::Pr2).then(crate::taa_reconstruct_gpu::WithoutDilation::new);
    let _unclipped = matches!(path, Path::Pr2).then(crate::taa_reconstruct_gpu::WithoutClip::new);
    let rig = SceneRig::new(gpu, render, scene::wgsl());
    let (mut approach, mut recession) = ((0.0, 0.0), (0.0, 0.0));
    for frame in 0..FRAMES {
        let half = scene::half(frame);
        let bar = [
            half as f32,
            scene::rate(frame) as f32,
            scene::depth(half) as f32,
            0.0,
        ];
        rig.frame(
            frame,
            !matches!(path, Path::Reset) && frame > 0,
            (bar, Some(static_camera())),
        );
        for (end, sum) in [
            (APPROACH_END, &mut approach),
            (RECESSION_END, &mut recession),
        ] {
            if frame + MEASURED > end && frame <= end {
                let image = rig.image();
                sum.0 += error(&image, &ring(end), half);
                sum.1 += error(&image, &edge_band(frame), half);
            }
        }
    }
    let window = |(ring, edge): (f64, f64)| Window {
        ring: ring / f64::from(u32::try_from(MEASURED).unwrap()),
        edge: edge / f64::from(u32::try_from(MEASURED).unwrap()),
    };
    Measured {
        approach: window(approach),
        recession: window(recession),
    }
}

fn measure(millionths: u32) -> (Measured, Measured, Measured) {
    let gpu = crate::gpu_tests::gpu();
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(OUTPUT)
        .unwrap();
    let all = (
        run(&gpu, render, Path::New),
        run(&gpu, render, Path::Pr2),
        run(&gpu, render, Path::Reset),
    );
    eprintln!(
        "scale {millionths} R {render:?}\n  new   {:?}\n  pr2   {:?}\n  reset {:?}",
        all.0, all.1, all.2
    );
    all
}

/// Slack over the spatial reconstruction when history is rejected: above the 8 to 10 % spread
/// between devices, and above the 6 % by which the rejecting path measured over the reset path
/// in the edge band of the recession.
const OVER_RESET: f64 = 1.2;

fn assert_toward_and_away(millionths: u32) {
    let (new, pr2, reset) = measure(millionths);
    for m in [new, pr2, reset] {
        for w in [m.approach, m.recession] {
            assert!(w.ring.is_finite() && w.ring > 0.0 && w.edge.is_finite() && w.edge > 0.0);
        }
    }
    // Receding: the strip is uncovered. The PR 2 path keeps a trail (measured at 1.6 to 1.9
    // times the reset ring and 1.8 to 2.1 times the reset edge band); the new path is held to
    // the target of the disocclusion test: half of the PR 2 excess over the reset floor.
    assert!(pr2.recession.ring >= 1.3 * reset.recession.ring);
    for (new, pr2, reset) in [
        (new.recession.ring, pr2.recession.ring, reset.recession.ring),
        (new.recession.edge, pr2.recession.edge, reset.recession.edge),
    ] {
        assert!(new <= reset + 0.5 * (pr2 - reset), "{new} {pr2} {reset}");
    }
    // Approaching: the bar is a uniform colour inside the checker range, so the colour clip makes
    // the PR 2 history exact inside it, and the new path loses the accumulated edge pixels
    // where it rejects history of a changing depth (measured up to 20 % over the PR 2 path, in
    // the ring and the band). Rejection converges to the spatial reconstruction, so the bound is
    // the reset error with the slack above; the PR 2 error is not a bound here.
    for (new, reset) in [
        (new.approach.ring, reset.approach.ring),
        (new.approach.edge, reset.approach.edge),
    ] {
        assert!(new <= OVER_RESET * reset, "{new} {reset}");
    }
}

#[test]
fn half_scale_toward_and_away() {
    assert_toward_and_away(500_000);
}

#[test]
fn two_thirds_scale_toward_and_away() {
    assert_toward_and_away(670_000);
}

#[test]
fn three_quarters_scale_toward_and_away() {
    assert_toward_and_away(770_000);
}
