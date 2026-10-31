//! The temporal reconstruction stage on the GPU, compared with `temporal_upscale_reference`.
//!
//! The stage reads `Rgba16Float` color and history and writes `Rgba16Float`, so the reference
//! gets the f16-quantised inputs exactly as the GPU loads them. Per output channel the bound is
//! the sum of these terms, each derived here (u = 2^-24 is the f32 unit roundoff):
//!
//! * Half store, `HALF_REL * |reference| + 2^-24`: Vulkan lets the f32 to f16 store round to
//!   nearest or toward zero (RADV truncates), so a stored value is within one f16 step, 2^-10
//!   relative, in either direction. Subnormal f16 spacing is 2^-24 absolute.
//! * The 16-tap weighted sum in f32, `gamma_16 * sum|w c| / |sum w|` with
//!   `gamma_16 = 16u / (1 - 16u)`, plus one division rounding `u |result|`.
//! * The Lanczos2 weights. The shader evaluates them without builtin transcendentals as
//!   `(sin a / a)^2 cos a`, `a = pi x / 2`, two degree-10 Taylor polynomials in `a^2`. Truncation
//!   is below pi^22 / 22! = 8e-11. Horner rounding is at most `2n u * sum|terms|` with n = 11 and
//!   sum|terms| bounded by cosh(pi) = 11.6 for cos and sinh(pi)/pi = 3.7 for sin a / a, so a
//!   1D weight is off by at most 22 * 6e-8 * (11.6 + 2 * 3.7) = 2.5e-5, taken as 3e-5. A 2D
//!   weight is the product of two: at most `2 * 3e-5 + 3e-5^2 + u`. Because the sum is
//!   normalised, a weight error `d` moves the result by `d * sum|c_i - result| / |sum w|`.
//! * The sample position `q = fma(u, R, jitter) - 0.5` carries at most `4u (max(R) + 1)` pixels
//!   of error, which moves the result by the local slope: at most 2.5 times the range of the
//!   16 taps (the sum of |L'| over the taps is below 2.5).
//! * Catmull-Rom history, when history is in play: 16 exact texel loads with polynomial
//!   weights. 1D weights have degree 3, coefficients up to 2.5, so at most `6u * 5 = 3e-7`
//!   error each, 2D at most 1e-6, giving `(gamma_16 * 1.5625 + 16 * 1e-6) * max|history|`
//!   (the absolute 1D weights sum to at most 1.25), below 3.5e-5 * max|history|. The
//!   blend then compresses, clips in `YCoCg` and decompresses: about 100 f32 operations with the
//!   final `1 / (1 - luma)` at most 5 for the test colors (compressed luma below 0.8), so 512u
//!   relative to the largest magnitude involved, and an absolute floor of 64u.
//!
//! These bounds hold for the lavapipe CPU rasteriser; they do not establish behavior on a
//! hardware GPU with different transcendental or rounding behavior, which this shader
//! largely avoids by using only add, multiply, fma, divide and sqrt.
use wgpu::util::DeviceExt;

use crate::cas_reference::Image;
use crate::taa_gpu_test_support::{
    SHADER, f16_bits, f16_to_f32, noisy, passthrough_pipeline, read, sampled, view, write_texture,
};
use crate::taa_upscale_gpu::{self, Inputs};
use crate::temporal_upscale_reference::Temporal;
use crate::{
    FrameConfig, FrameTargets, PostChain, PostError, Resources, Stage, StageFactory, StageId,
    StageSet, StageTarget, TaaSettings, TemporalBlock, VELOCITY_FORMAT, color_texture,
};

const U: f64 = 1.0 / 16_777_216.0;
const HALF_REL: f64 = 1.0 / 1024.0;
const GAMMA16: f64 = 16.0 * U / (1.0 - 16.0 * U);
const LANCZOS_1D: f64 = 3e-5;
const LANCZOS_2D: f64 = 2.0 * LANCZOS_1D + LANCZOS_1D * LANCZOS_1D + U;
const HISTORY_REL: f64 = 3.5e-5;
const CHAIN_REL: f64 = 512.0 * U;
const ABS_FLOOR: f64 = 64.0 * U;

#[derive(Clone)]
struct Case {
    color: Image,
    history: Image,
    jitter: [f32; 2],
    motion: [f32; 2],
    valid: bool,
    weight: f32,
}
impl Case {
    fn new(color: &Image, history: &Image) -> Self {
        Self {
            color: quantise(color),
            history: quantise(history),
            jitter: [0.3125, -0.4375],
            motion: [0.0; 2],
            valid: true,
            weight: 0.9,
        }
    }

    fn output(&self) -> [u32; 2] {
        self.history.size.map(|v| u32::try_from(v).unwrap())
    }

    fn settings(&self) -> TaaSettings {
        TaaSettings::new(self.weight)
    }

    fn quantised_motion(&self) -> [f64; 2] {
        self.motion.map(|v| f64::from(f16_to_f32(f16_bits(v))))
    }

    fn reference(&self) -> Temporal<'_> {
        Temporal {
            color: &self.color,
            history: &self.history,
            output: self.history.size,
            jitter: self.jitter.map(f64::from),
            history_valid: self.valid,
            settings: self.settings(),
            motion_uv: self.quantised_motion(),
        }
    }
}

fn quantise(image: &Image) -> Image {
    Image {
        size: image.size,
        pixels: image
            .pixels
            .iter()
            .map(|p| p.map(|v| f16_to_f32(f16_bits(v))))
            .collect(),
    }
}

fn flat(size: [i32; 2], v: f32) -> Image {
    Image::from_fn(size, |_, _| [v, v, v, 0.5])
}

fn ramp(size: [i32; 2]) -> Image {
    Image::from_fn(size, |x, y| {
        let v = 0.25 + 0.125 * f32::from(i16::try_from(x).unwrap());
        let w = 0.25 + 0.0625 * f32::from(i16::try_from(y).unwrap());
        [v, v + w, w, 0.5]
    })
}

fn bytes16(pixels: &[[f32; 4]]) -> Vec<u8> {
    pixels
        .iter()
        .flatten()
        .flat_map(|v| f16_bits(*v).to_le_bytes())
        .collect()
}

struct Stage1 {
    color: wgpu::TextureView,
    motion: wgpu::TextureView,
    uniform: wgpu::Buffer,
    history: wgpu::Texture,
    output: wgpu::Texture,
}
impl StageFactory for Stage1 {
    fn build(
        &self,
        device: &wgpu::Device,
        _: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set = StageSet::default();
        taa_upscale_gpu::append(
            device,
            &mut set,
            &Inputs {
                color: &self.color,
                motion: &self.motion,
                uniform: &self.uniform,
                history: &self.history,
                output: &self.output,
            },
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("temporal upscale test passes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline =
            passthrough_pipeline(device, &module, "identity", Some(taa_upscale_gpu::FORMAT));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view(&self.output)),
            }],
        });
        set.stages.push(Stage::fullscreen(
            StageId::new("temporal upscale test output", 0),
            &pipeline,
            &[&group],
            StageTarget::Output,
        ));
        Ok(set)
    }
}

/// Runs the stage on lavapipe and reads the output texture.
fn run(case: &Case) -> Vec<[f64; 4]> {
    let (device, queue) = crate::gpu_tests::gpu();
    let render = case.color.size.map(|v| u32::try_from(v).unwrap());
    let output = case.output();
    let color = sampled(&device, "color", render, taa_upscale_gpu::FORMAT);
    write_texture(&queue, &color, &bytes16(&case.color.pixels), 8);
    let motion = sampled(&device, "motion", render, VELOCITY_FORMAT);
    let texel: Vec<u8> = case
        .motion
        .iter()
        .flat_map(|v| f16_bits(*v).to_le_bytes())
        .collect();
    let field: Vec<u8> = texel
        .iter()
        .copied()
        .cycle()
        .take(texel.len() * case.color.pixels.len())
        .collect();
    write_texture(&queue, &motion, &field, 4);
    let history = taa_upscale_gpu::history(&device, output);
    write_texture(&queue, &history, &bytes16(&case.history.pixels), 8);
    let block = TemporalBlock {
        render,
        output,
        jitter: case.jitter,
        phase_count: 8,
        settings: case.settings(),
        history_valid: case.valid,
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("temporal block"),
        contents: &block.to_bytes().unwrap(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let target = taa_upscale_gpu::output(&device, output);
    let chain = PostChain::new(
        &device,
        FrameConfig::new(output, taa_upscale_gpu::FORMAT, 1),
        Resources::default(),
        Stage1 {
            color: view(&color),
            motion: view(&motion),
            uniform,
            history,
            output: target.clone(),
        },
    )
    .unwrap();
    let sink = color_texture(&device, "sink", output, taa_upscale_gpu::FORMAT);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(&mut encoder, &view(&sink));
    queue.submit([encoder.finish()]);
    read((&device, &queue), &target)
}

/// Largest absolute difference between horizontally or vertically adjacent texels.
fn gradient(image: &Image) -> f64 {
    let mut worst: f64 = 0.0;
    for y in 0..image.size[1] {
        for x in 0..image.size[0] {
            for (dx, dy) in [(1, 0), (0, 1)] {
                let (a, b) = (image.load(x, y), image.load(x + dx, y + dy));
                worst = a
                    .iter()
                    .zip(b)
                    .fold(worst, |m, (a, b)| m.max((a - b).abs()));
            }
        }
    }
    worst
}

/// The derived per-channel bound at output pixel `at`; see the module documentation.
fn bound(case: &Case, at: [i32; 2], expected: [f64; 4]) -> [f64; 4] {
    let t = case.reference();
    let taps = t.footprint(at);
    let total: f64 = taps.iter().map(|(w, _)| w).sum();
    let spatial = t.reconstruct(at);
    let extent = f64::from(case.color.size[0].max(case.color.size[1]) + 1);
    let position = 4.0 * U * extent;
    let history = case.valid && case.weight > 0.0;
    let hmax = case
        .history
        .pixels
        .iter()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(f64::from(*v)));
    let hpos = 4.0 * U * f64::from(case.history.size[0].max(case.history.size[1]) + 1);
    std::array::from_fn(|k| {
        let abs_sum: f64 = taps.iter().map(|(w, c)| (w * c[k]).abs()).sum();
        let deviation: f64 = taps.iter().map(|(_, c)| (c[k] - spatial[k]).abs()).sum();
        let (lo, hi) = taps.iter().fold((f64::MAX, f64::MIN), |(lo, hi), (_, c)| {
            (lo.min(c[k]), hi.max(c[k]))
        });
        let stage = GAMMA16 * abs_sum / total.abs()
            + U * spatial[k].abs()
            + 1.01 * LANCZOS_2D * deviation / total.abs()
            + position * 2.5 * (hi - lo);
        let blend = if history {
            let magnitude = expected[k].abs().max(hmax).max(spatial[k].abs());
            5.0 * HISTORY_REL * hmax + CHAIN_REL * magnitude + hpos * 2.5 * gradient(&case.history)
        } else {
            0.0
        };
        HALF_REL * expected[k].abs() + U + stage + blend + ABS_FLOOR
    })
}

fn expected_image(case: &Case) -> Vec<[f64; 4]> {
    let t = case.reference();
    let [w, h] = case.history.size;
    (0..h)
        .flat_map(|y| (0..w).map(move |x| [x, y]))
        .map(|at| t.resolve_pixel(at))
        .collect()
}

fn assert_matches_reference(case: &Case, context: &str) -> Vec<[f64; 4]> {
    let actual = run(case);
    let expected = expected_image(case);
    let width = case.history.size[0];
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
        let at = [
            i32::try_from(index).unwrap() % width,
            i32::try_from(index).unwrap() / width,
        ];
        let tolerance = bound(case, at, *e);
        for k in 0..4 {
            assert!(
                (a[k] - e[k]).abs() <= tolerance[k],
                "{context}: pixel {at:?} channel {k}: gpu {} reference {} bound {}",
                a[k],
                e[k],
                tolerance[k]
            );
        }
    }
    actual
}

const R: [i32; 2] = [13, 7];
const O: [i32; 2] = [20, 11];

#[test]
fn a_constant_field_is_reproduced_at_the_output_extent() {
    let mut case = Case::new(&flat(R, 0.5), &flat(O, 0.25));
    case.weight = 0.0;
    let out = assert_matches_reference(&case, "constant");
    assert_eq!(out.len(), 220);
    for pixel in out {
        // Red and alpha both come from a normalised f32 sum that can land just below 0.5, and a
        // truncating half store then reads one f16 step low, so both get the half-store bound.
        let bound = HALF_REL * 0.5 + 1e-6;
        assert!((pixel[0] - 0.5).abs() <= bound && (pixel[3] - 0.5).abs() <= bound);
    }
}

#[test]
fn a_ramp_matches_the_reference_including_the_lanczos_shift() {
    for jitter in [[0.3125, -0.4375], [-0.1875, 0.0625], [0.0, 0.0]] {
        let mut case = Case::new(&ramp(R), &flat(O, 0.25));
        case.weight = 0.0;
        case.jitter = jitter;
        assert_matches_reference(&case, &format!("ramp {jitter:?}"));
    }
}

#[test]
fn the_jitter_sign_moves_the_sample_toward_later_texels() {
    let mut case = Case::new(&ramp(R), &flat(O, 0.25));
    case.weight = 0.0;
    case.jitter = [0.0, 0.0];
    let base = run(&case);
    case.jitter = [0.375, 0.0];
    let shifted = assert_matches_reference(&case, "positive jitter");
    let width = usize::try_from(O[0]).unwrap();
    for y in 2..9 {
        for x in 4..16 {
            let i = y * width + x;
            assert!(shifted[i][0] > base[i][0], "pixel {x},{y}");
        }
    }
}

#[test]
fn history_weight_zero_and_invalid_history_both_equal_the_spatial_reconstruction() {
    let mut zero = Case::new(&noisy(R, 0), &noisy(O, 9));
    zero.weight = 0.0;
    let spatial = assert_matches_reference(&zero, "weight 0");
    let mut invalid = zero.clone();
    invalid.weight = 0.9;
    invalid.valid = false;
    assert_eq!(
        assert_matches_reference(&invalid, "invalid history"),
        spatial
    );
}

#[test]
fn odd_extents_with_valid_history_match_the_reference() {
    let case = Case::new(&noisy(R, 0), &noisy(O, 9));
    assert_matches_reference(&case, "13x7 to 20x11 with history");
}

#[test]
fn motion_moves_the_history_lookup_and_fast_motion_or_leaving_the_image_drops_it() {
    let mut case = Case::new(&noisy(R, 0), &ramp(O));
    case.motion = [0.015_625, -0.007_812_5];
    assert_matches_reference(&case, "small motion");
    let mut spatial = case.clone();
    spatial.weight = 0.0;
    let spatial_out = run(&spatial);
    case.motion = [0.25, 0.0];
    assert_eq!(assert_matches_reference(&case, "fast motion"), spatial_out);
    case.motion = [0.0, -0.9375];
    assert_eq!(
        assert_matches_reference(&case, "history outside the image"),
        spatial_out
    );
}
