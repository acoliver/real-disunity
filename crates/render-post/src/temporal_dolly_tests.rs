//! The camera term with real depth on lavapipe. A checkered plane leans away to the right and the
//! camera moves along its view axis (`temporal_dolly_scene`), so a dolly step moves every pixel
//! toward the centre of the frame by an amount that depends on the distance of the surface at
//! that pixel: the reprojection reads the depth the rasteriser wrote. The object residual is zero.
//!
//! The uploaded reprojection maps (NDC x, NDC y, device depth, 1) of this frame to the previous
//! clip space. With device depth `z = A + B / d` and the previous camera `STEP` further back
//! (`d' = d + STEP`), the previous clip position divided by `d` is `(x, y, A + (A STEP + B)(z - A)
//! / B, 1 + STEP (z - A) / B)`, linear in `z`, which gives the two columns below. Errors are the
//! mean absolute HDR error against the 4x4 box filtered f64 scene of each frame's camera over
//! the last `MEASURED` frames of `CYCLES` jitter cycles, away from the frame border.
//!
//! "Without depth" is the same reprojection evaluated at device depth 0 (the near plane) for
//! every pixel: one constant distance in place of the per-pixel distances of the plane.
use crate::TemporalCamera;
use crate::temporal_dolly_scene as scene;
use crate::temporal_scene_test_rig::{OUTPUT, SceneRig};

/// Camera advance per frame in plane units.
const STEP: f64 = 0.01;
const CYCLES: u64 = 2;
/// Last frames averaged.
const MEASURED: u64 = 8;
/// Pixels from the frame border excluded from the error.
const BORDER: u32 = 4;

/// The reprojection of a dolly step of `STEP`, column-major.
fn dolly_camera() -> TemporalCamera {
    let (a, b) = (scene::DEPTH_A, scene::DEPTH_B);
    let mut reprojection = [[0.0; 4]; 4];
    reprojection[0][0] = 1.0;
    reprojection[1][1] = 1.0;
    reprojection[2][2] = (a * STEP + b) / b;
    reprojection[2][3] = STEP / b;
    reprojection[3][2] = a - a * (a * STEP + b) / b;
    reprojection[3][3] = 1.0 - a * STEP / b;
    TemporalCamera {
        reprojection,
        depth_terms: [a, b],
        valid: true,
    }
}

fn truth(pixel: [u32; 2], advance: f64) -> f64 {
    const SUB: u32 = 4;
    let at = |v: u32, sub: u32| f64::from(v) + (f64::from(sub) + 0.5) / f64::from(SUB);
    let mut sum = 0.0;
    for j in 0..SUB {
        for i in 0..SUB {
            sum += scene::value([at(pixel[0], i), at(pixel[1], j)], advance);
        }
    }
    sum / f64::from(SUB * SUB)
}

fn error(image: &[f64], advance: f64) -> f64 {
    let width = usize::try_from(OUTPUT[0]).unwrap();
    let (mut sum, mut count) = (0.0, 0_u32);
    for y in BORDER..OUTPUT[1] - BORDER {
        for x in BORDER..OUTPUT[0] - BORDER {
            let i = usize::try_from(y).unwrap() * width + usize::try_from(x).unwrap();
            sum += (image[i] - truth([x, y], advance)).abs();
            count += 1;
        }
    }
    sum / f64::from(count)
}

/// How a dolly run treats the camera and the history.
#[derive(Clone, Copy)]
enum Run {
    /// History kept, camera term with the depth the rasteriser wrote.
    Depth,
    /// History kept, the camera term evaluated at device depth 0 for every pixel.
    ZeroedDepth,
    /// History reset every frame: the spatial reconstruction alone.
    Reset,
}

fn run(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], mode: Run) -> f64 {
    let rig = SceneRig::new(gpu, render, scene::wgsl());
    let frames = CYCLES * u64::from(rig.jitter.length());
    let mut sum = 0.0;
    for index in 0..frames {
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame indices of a test sequence are far below 2^52"
        )]
        let advance = STEP * index as f64;
        #[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
        let tail = [advance as f32, 0.0, 0.0, 0.0];
        let camera = match mode {
            Run::Depth | Run::Reset => dolly_camera(),
            Run::ZeroedDepth => zeroed_depth(dolly_camera()),
        };
        rig.frame(
            index,
            !matches!(mode, Run::Reset) && index > 0,
            (tail, Some(camera)),
        );
        if index >= frames - MEASURED {
            sum += error(&rig.image(), advance) / f64::from(u32::try_from(MEASURED).unwrap());
        }
    }
    sum
}

/// The camera with its depth column removed: every pixel is reprojected as if its device depth
/// were 0.
fn zeroed_depth(mut camera: TemporalCamera) -> TemporalCamera {
    camera.reprojection[2] = [0.0; 4];
    camera
}

struct Row {
    depth: f64,
    zeroed: f64,
    reset: f64,
}

fn measure(render: [u32; 2]) -> Row {
    let gpu = crate::gpu_tests::gpu();
    let row = Row {
        depth: run(&gpu, render, Run::Depth),
        zeroed: run(&gpu, render, Run::ZeroedDepth),
        reset: run(&gpu, render, Run::Reset),
    };
    eprintln!(
        "R {render:?}: depth {:.5} zeroed depth {:.5} reset {:.5} | depth/reset {:.3} zeroed/depth {:.3}",
        row.depth,
        row.zeroed,
        row.reset,
        row.depth / row.reset,
        row.zeroed / row.depth
    );
    row
}

/// The reprojection that reads the rasterised depth keeps the history aligned, so its error is
/// below the spatial floor. Ignoring the depth moves every pixel by the wrong amount and the
/// error is that of the floor or worse.
///
/// Measured on lavapipe: depth over reset 0.69 (R 80x48) and 0.62 (107x64); zeroed depth over
/// depth 1.44 and 1.62. With the shader ignoring the depth texture both ratios are 0.999 and
/// 1.000 and the tests fail. The limits leave 0.2 and 0.29, more than the 8 to 10% platform
/// spread.
const DEPTH_OVER_RESET: f64 = 0.9;
const ZEROED_OVER_DEPTH: f64 = 1.15;

fn assert_depth_matters(row: &Row) {
    assert!(row.depth.is_finite() && row.zeroed.is_finite() && row.reset.is_finite());
    assert!(
        row.depth <= DEPTH_OVER_RESET * row.reset,
        "with depth {} is not below the spatial floor {}",
        row.depth,
        row.reset
    );
    assert!(
        row.zeroed >= ZEROED_OVER_DEPTH * row.depth,
        "zeroed depth {} is not measurably worse than {}",
        row.zeroed,
        row.depth
    );
}

#[test]
fn a_dolly_over_a_leaning_plane_reprojects_with_the_depth_of_each_pixel() {
    assert_depth_matters(&measure([80, 48]));
}

#[test]
fn a_dolly_reprojects_with_depth_at_unequal_extents() {
    assert_depth_matters(&measure([107, 64]));
}
