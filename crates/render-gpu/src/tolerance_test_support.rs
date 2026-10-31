//! Bounds for comparing 8-bit display output of GPU renders, shared by the render crates' tests.
//! Compiled only for tests and with the `test-support` feature.
//!
//! GPU float arithmetic is not bit-exact across drivers, so a render is never compared byte for
//! byte with a CPU computation or with a different shader. Byte identity is kept only between
//! two runs of the same pipeline on the same device.
//!
//! The reference is compared before quantisation, in output steps (1/255). The conversion to a
//! normalised 8-bit value rounds to nearest, 0.5 step. Vulkan leaves the precision of the sRGB
//! transfer to the implementation, so 0.1 step is allowed for it. The f32 arithmetic before the
//! store errs by at most `2 * 64 * 2^-24` relative (GPU and CPU, a conservative 64 operations),
//! multiplied by the steepest slope of the encoding (12.92 in the sRGB toe, 255 steps) and by a
//! colour range of at most 1: `2 * 64 * 2^-24 * 12.92 * 255 = 0.025` steps. The bound is
//! `0.5 + 0.1 + 0.025 = 0.625` steps. A half-float store (`2^-10` relative, see the half bound
//! in render-post's `fxaa_tolerance_tests`) adds [`HALF_INTERMEDIATE_STEPS`] where one exists.

/// Rounding to nearest (0.5), implementation sRGB transfer precision (0.1), f32 arithmetic
/// before the conversion (0.025), in 8-bit output steps.
pub const OUTPUT_STEPS: f64 = 0.625;

/// Unquantised encoded value of a display value, in output steps.
pub fn encoded_steps(value: f64, srgb_color: bool) -> f64 {
    255.0
        * if !srgb_color {
            value
        } else if value <= 0.003_130_8 {
            12.92 * value
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
}

/// Largest error, in output steps, of storing a display value once as `Rgba16Float` before the
/// sRGB attachment encodes it. The store errs by at most `2^-10 d` for a value `d` (a whole unit
/// in the last place, see the module comment). The encoding's steepest `d * ds/dd` is at `d = 1`
/// in the power segment: `255 * 1.055 / 2.4 * 1 = 112.1` steps, so the error is at most
/// `112.1 / 1024 = 0.1095`. The toe (`d <= 0.0031308`, slope `12.92 * 255`) gives only 0.01.
pub const HALF_INTERMEDIATE_STEPS: f64 = 255.0 * 1.055 / 2.4 / 1024.0;

/// Largest difference, in output steps, between two 8-bit renders of the same display value
/// where one passes the plain tone path and the other an extra half intermediate (SMAA, CMAA2,
/// CAS). Each render is within [`OUTPUT_STEPS`] of the unquantised value; the second adds
/// [`HALF_INTERMEDIATE_STEPS`] for its extra store. Integer codes then differ by at most
/// `floor(0.625 + 0.625 + 0.1095) = 1`.
pub const PATH_STEPS: f64 = 2.0 * OUTPUT_STEPS + HALF_INTERMEDIATE_STEPS;

/// Asserts two 8-bit readbacks of the same scene from different shader paths agree within
/// [`PATH_STEPS`] in every channel. Byte identity is not required: the paths round and store
/// differently.
pub fn assert_paths_agree(plain: &[u8], other: &[u8], context: &str) {
    assert_eq!(plain.len(), other.len(), "{context}");
    for (index, (&a, &b)) in plain.iter().zip(other).enumerate() {
        assert!(
            f64::from(a.abs_diff(b)) <= PATH_STEPS,
            "{context}: channel {index}, {a} versus {b}, allowed {PATH_STEPS}"
        );
    }
}

/// Asserts an 8-bit `code` is within [`OUTPUT_STEPS`] plus `extra` of an unquantised reference
/// in output steps.
pub fn assert_code_within(code: u8, reference: f64, extra: f64, context: &str) {
    let bound = OUTPUT_STEPS + extra;
    assert!(
        (f64::from(code) - reference).abs() <= bound,
        "{context}: got {code}, want {reference}, bound {bound}"
    );
}

/// Unit roundoff of f32 (`2^-24`) times a conservative 64 operations, twice (GPU and CPU).
pub const ARITHMETIC: f64 = 2.0 * 64.0 / 16_777_216.0;

/// Largest error of a half-float store of `reference` computed from texels up to `range`.
/// Vulkan leaves the float conversion rounding to the implementation, so the stored value is one
/// of the two nearest halves and the error is a whole unit in the last place, at most
/// `2^-10 |v|` (11-bit significand). Below the smallest normal half the spacing is `2^-24`. The
/// f32 arithmetic before the store adds [`ARITHMETIC`] times the largest input.
pub fn half_bound(reference: f64, range: f64) -> f64 {
    reference.abs() / 1024.0 + 2.0_f64.powi(-24) + ARITHMETIC * range
}
