//! A textured Unity pass drawn with a constant-buffer tint and two keyword variants.

use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::runtime::TargetImage;

use crate::common::{opaque_state, runtime};
use crate::rig::{
    ADD, MULTIPLY, OFFSET_VALUE, Pass, QUAD, Samplers, Scene, TEXELS, TINT_VALUE, pass, properties,
};
use worldrunner_render_program::state::resolve_state;

/// The quad covers the 2x2 target and each pixel center samples a texel center (uv 0.25 or
/// 0.75), so the linear filter returns that texel and no neighbor.
fn texel_at(x: u32, y: u32) -> [u8; 4] {
    TEXELS[(y * 2 + x) as usize]
}

/// Asserts every pixel is within the shared 8-bit bound of the unquantised `predict(texel)`,
/// in 8-bit steps.
fn assert_pixels(image: &TargetImage, predict: impl Fn([f64; 4]) -> [f64; 4], context: &str) {
    for y in 0..2 {
        for x in 0..2 {
            let texel = texel_at(x, y).map(f64::from);
            let want = predict(texel);
            for (channel, (&code, reference)) in image.pixel(x, y).iter().zip(want).enumerate() {
                assert_code_within(
                    code,
                    reference.clamp(0.0, 255.0),
                    0.0,
                    &format!("{context} pixel ({x}, {y}) channel {channel}"),
                );
            }
        }
    }
}

fn draw(pass: &Pass, enabled: &[u16]) -> TargetImage {
    let mut runtime = runtime();
    let (material, globals) = properties();
    let state = resolve_state(&opaque_state(), &[]).unwrap().state;
    let rendered = crate::rig::render(
        &mut runtime,
        &Scene {
            pass,
            enabled,
            material: &material,
            globals: &globals,
            state: &state,
            indices: &QUAD,
        },
    );
    assert!(
        rendered.missing.is_empty(),
        "every property is set: {:?}",
        rendered.missing
    );
    rendered.image
}

#[test]
fn the_multiply_variant_scales_each_texel_by_the_tint() {
    let image = draw(&pass(Samplers::Separate), &[MULTIPLY]);
    assert_pixels(
        &image,
        |texel| std::array::from_fn(|c| texel[c] * f64::from(TINT_VALUE[c])),
        "multiply",
    );
}

#[test]
fn the_add_variant_adds_the_offset_to_the_color_and_keeps_the_texel_alpha() {
    let image = draw(&pass(Samplers::Separate), &[ADD]);
    assert_pixels(
        &image,
        |texel| {
            std::array::from_fn(|c| {
                if c == 3 {
                    texel[c]
                } else {
                    texel[c] + 255.0 * f64::from(OFFSET_VALUE[c])
                }
            })
        },
        "add",
    );
}

#[test]
fn a_split_combined_sampler_binds_its_texture_and_sampler_and_samples_the_texel() {
    // The texture sits at prepared binding 1 and the sampler the importer split off at 2,
    // with the draw constants moved to 3; the pass is otherwise the multiply variant.
    let image = draw(&pass(Samplers::Split), &[MULTIPLY]);
    assert_pixels(
        &image,
        |texel| std::array::from_fn(|c| texel[c] * f64::from(TINT_VALUE[c])),
        "split sampler",
    );
}
