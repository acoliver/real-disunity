//! Comparison of FXAA output with a CPU reference within bounds derived from the storage and
//! output formats. GPU float arithmetic is not bit-exact across drivers, so GPU output is never
//! compared byte for byte with a CPU computation. Byte identity is kept only between two runs
//! of the same pipeline on the same device.
//!
//! Half-float output. The shader computes in f32 and stores `Rgba16Float`. Vulkan leaves the
//! rounding of the float conversion to the implementation: the stored value is one of the two
//! nearest halves, so round-to-nearest errs by half a unit in the last place and round-toward-
//! zero by a whole one (lavapipe was observed to truncate). A half has an 11-bit significand,
//! so one unit in the last place is at most `2^-10 * |v|`. Below the smallest normal half
//! (`2^-14`) the spacing is the fixed `2^-24`. The f32 arithmetic before the store (tangent
//! normalisation, two bilinear taps of four fused terms each, two mixes) is bounded by 64
//! rounding operations of relative size `2^-24` against the largest texel `R` in the fixture,
//! because every weight is in 0..=1. A conservative 64 covers a different fusing or operation
//! order. The CPU reference is f32 as well and adds the same term again. The bound is
//! `2^-10 |v| + 2^-24 + 2 * 64 * 2^-24 * R`. The filter taps use integer loads and explicit
//! arithmetic, so Vulkan's implementation-defined texture filtering precision does not apply.
//!
//! 8-bit output. The reference is compared before quantisation, in output steps (1/255). The
//! conversion to a normalised 8-bit value rounds to nearest, 0.5 step. Vulkan leaves the
//! precision of the sRGB transfer to the implementation, so 0.1 step is allowed for it. The
//! f32 arithmetic term above is multiplied by the steepest slope of the encoding (12.92 in the
//! sRGB toe, 255 steps) and by `R <= 1`: `2 * 64 * 2^-24 * 12.92 * 255 = 0.025` steps. The
//! bound is `0.5 + 0.1 + 0.025 = 0.625` steps.
use super::filter_reference::Image;
use crate::tone_map_gpu_tests::half;

/// Rounding to nearest (0.5), implementation sRGB transfer precision (0.1), f32 arithmetic
/// before the conversion (0.025), in 8-bit output steps.
pub(super) const OUTPUT_STEPS: f64 = 0.625;
/// Unit roundoff of f32 (`2^-24`) times a conservative 64 operations, twice (GPU and CPU).
const ARITHMETIC: f64 = 2.0 * 64.0 / 16_777_216.0;

/// Largest absolute colour texel of an image, the scale of its f32 arithmetic error. Alpha is
/// copied, not computed, so it does not enter the arithmetic term.
pub(super) fn range(image: &Image) -> f64 {
    image
        .pixels
        .iter()
        .flat_map(|pixel| &pixel[..3])
        .map(|&v| f64::from(v).abs())
        .fold(0.0, f64::max)
}

/// Largest error of a half-float store of `reference` computed from texels up to `range`.
pub(super) fn half_bound(reference: f64, range: f64) -> f64 {
    reference.abs() / 1024.0 + 2.0_f64.powi(-24) + ARITHMETIC * range
}

pub(super) fn decode_half(bytes: &[u8]) -> Vec<f64> {
    bytes.chunks_exact(2).map(half).collect()
}

/// Whether every channel of half-float `actual` is within its bound of `expected`.
pub(super) fn half_within(actual: &[u8], expected: &Image, range: f64) -> bool {
    let reference = expected.pixels.iter().flatten();
    let actual = decode_half(actual);
    actual.len() == expected.pixels.len() * 4
        && actual.iter().zip(reference).all(|(&got, &want)| {
            let want = f64::from(want);
            (got - want).abs() <= half_bound(want, range)
        })
}

/// Asserts every channel of half-float `actual` is within its bound of `expected`.
pub(super) fn assert_half_close(actual: &[u8], expected: &Image, range: f64, context: &str) {
    let decoded = decode_half(actual);
    let reference = expected.pixels.iter().flatten();
    assert_eq!(decoded.len(), expected.pixels.len() * 4, "{context}");
    for (index, (&got, &want)) in decoded.iter().zip(reference).enumerate() {
        let want = f64::from(want);
        let bound = half_bound(want, range);
        assert!(
            (got - want).abs() <= bound,
            "{context}: channel {index}, got {got}, want {want}, bound {bound}"
        );
    }
}

/// Encoded value in output steps, unquantised.
pub(super) fn steps(value: f32, srgb_color: bool) -> f64 {
    let value = f64::from(value);
    255.0
        * if !srgb_color {
            value
        } else if value <= 0.003_130_8 {
            12.92 * value
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
}

/// Asserts 8-bit `actual` is within [`OUTPUT_STEPS`] of the unquantised reference.
pub(super) fn assert_steps_close(
    actual: &[u8],
    expected: &Image,
    format: wgpu::TextureFormat,
    context: &str,
) {
    let srgb = format == wgpu::TextureFormat::Rgba8UnormSrgb;
    assert_eq!(actual.len(), expected.pixels.len() * 4, "{context}");
    let reference = expected.pixels.iter().flatten().enumerate();
    for ((index, &want), &got) in reference.zip(actual) {
        let want = steps(want, srgb && index % 4 < 3);
        assert!(
            (f64::from(got) - want).abs() <= OUTPUT_STEPS,
            "{context}: channel {index}, got {got}, want {want}"
        );
    }
}

/// Renders `entry` and checks the output against `expected` within the derived bound.
pub(super) fn run_within(
    gpu: (&wgpu::Device, &wgpu::Queue),
    entry: &str,
    inputs: (&Image, Option<&Image>),
    expected: &Image,
    format: wgpu::TextureFormat,
    constants: &[(&str, f64)],
) {
    let actual =
        super::filter_reference_gpu_tests::render(gpu, entry, inputs, expected, format, constants);
    let context = format!("{entry}, {format:?}");
    if format == wgpu::TextureFormat::Rgba16Float {
        assert_half_close(&actual, expected, range(inputs.0), &context);
    } else {
        assert_steps_close(&actual, expected, format, &context);
    }
}

/// Largest per-channel distance between two half-float readbacks of the same size.
pub(super) fn max_half_difference(a: &[u8], b: &[u8]) -> f64 {
    decode_half(a)
        .into_iter()
        .zip(decode_half(b))
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

/// Largest per-channel distance between two 8-bit readbacks, in steps.
pub(super) fn max_step_difference(a: &[u8], b: &[u8]) -> u8 {
    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap()
}

/// Pixels of 8-bit `a` and `b` that differ by more than `allowed` steps in any channel.
pub(super) fn differing_pixels(a: &[u8], b: &[u8], allowed: u8) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(a, b)| max_step_difference(a, b) > allowed)
        .count()
}

#[test]
fn the_half_bound_covers_the_store_and_the_arithmetic_and_nothing_wider() {
    // 0.75 stores within 0.75 / 1024; a unit-range fixture adds 128 / 2^24 = 7.6e-6.
    let bound = half_bound(0.75, 1.0);
    assert!((bound - (0.75 / 1024.0 + 2.0_f64.powi(-24) + 7.629_394_5e-6)).abs() < 1e-12);
    assert!(bound < 8.0e-4);
    // The bound stays well under one 8-bit step (1/255) at 0.75.
    assert!(bound * 255.0 < 0.25);
}

#[test]
fn the_output_bound_rejects_a_whole_step_error() {
    let expected = Image {
        size: [1, 1],
        pixels: vec![[0.25, 0.25, 0.25, 0.25]],
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;
    // 0.25 * 255 = 63.75: 64 is 0.25 away and 63 is 0.75 away.
    assert_steps_close(&[64, 64, 64, 64], &expected, format, "near");
    assert!(
        std::panic::catch_unwind(|| {
            assert_steps_close(&[63, 63, 63, 63], &expected, format, "far");
        })
        .is_err()
    );
}
