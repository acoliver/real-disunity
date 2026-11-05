//! Render state that material properties control, observed in the pixels of a drawn pass.

use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::runtime::TargetImage;
use worldrunner_render_program::state::resolve_state;

use crate::common::{named, opaque_state};
use crate::rig::{MULTIPLY, QUAD, Samplers, Scene, TEXELS, TINT_VALUE, pass, properties, render};

/// Unity's `BlendMode` numbers for `SrcAlpha` and `OneMinusSrcAlpha`, and `CullMode` for off.
const SRC_ALPHA: f32 = 5.0;
const ONE_MINUS_SRC_ALPHA: f32 = 10.0;
const CULL_OFF: f32 = 0.0;
const CULL_BACK: f32 = 2.0;

/// The target holds this before the draw; every channel is a whole 8-bit code (51, 102, 153,
/// 255) so the stored value is exactly the one predicted.
const BACKDROP: [f64; 4] = [0.2, 0.4, 0.6, 1.0];

/// The same pass with a cull and a blend that read the material, as a Unity shader with
/// `Cull [_Cull]` and `Blend [_SrcBlend] [_DstBlend]` has.
fn draw(controls: [(&str, f32); 3], indices: &[u16]) -> TargetImage {
    let (mut material, globals) = properties();
    for (name, value) in controls {
        material.set(name, PropertyValue::Float(value));
    }
    let mut shader_state = opaque_state();
    shader_state.depth.cull = named(CULL_BACK, "_Cull");
    shader_state.blend[0].src = named(1.0, "_SrcBlend");
    shader_state.blend[0].dst = named(0.0, "_DstBlend");
    let state = resolve_state(&shader_state, &[&material]).unwrap().state;
    let mut runtime = crate::common::runtime();
    render(
        &mut runtime,
        &Scene {
            pass: &pass(Samplers::Separate),
            enabled: &[MULTIPLY],
            material: &material,
            globals: &globals,
            state: &state,
            clear: BACKDROP,
            indices,
        },
    )
    .image
}

/// The multiply variant's color at a pixel, in 8-bit steps and unquantised.
fn shaded(x: u32, y: u32) -> [f64; 4] {
    let texel = TEXELS[(y * 2 + x) as usize];
    std::array::from_fn(|c| f64::from(texel[c]) * f64::from(TINT_VALUE[c]))
}

/// Extra steps allowed beyond the shared 8-bit bound for a blended store. Measured: the Radeon
/// 780M stays within the shared bound, but lavapipe's fixed-point 8-bit blend differs from the
/// round-to-nearest prediction by up to 0.8 steps (pixel (1, 0) red: stored 25 for a
/// prediction of 25.8; pixel (1, 0) blue: 66 for 66.6). Vulkan does not fix the precision of
/// blend arithmetic, so the allowance is the 0.5 that brings the bound to 1.125, one whole code.
const BLEND_EXTRA_STEPS: f64 = 0.5;

fn assert_pixels(
    image: &TargetImage,
    predict: impl Fn(u32, u32) -> [f64; 4],
    extra: f64,
    context: &str,
) {
    for y in 0..2 {
        for x in 0..2 {
            for (channel, (&code, want)) in image.pixel(x, y).iter().zip(predict(x, y)).enumerate()
            {
                assert_code_within(
                    code,
                    want,
                    extra,
                    &format!("{context} pixel ({x}, {y}) channel {channel}"),
                );
            }
        }
    }
}

/// The backdrop in 8-bit steps.
fn backdrop() -> [f64; 4] {
    BACKDROP.map(|c| c * 255.0)
}

#[test]
fn blend_one_zero_replaces_the_backdrop_with_the_shaded_color() {
    let image = draw(
        [("_Cull", CULL_BACK), ("_SrcBlend", 1.0), ("_DstBlend", 0.0)],
        &QUAD,
    );
    assert_pixels(&image, shaded, 0.0, "opaque");
}

#[test]
fn src_alpha_blending_mixes_the_shaded_color_into_the_backdrop_by_its_alpha() {
    let image = draw(
        [
            ("_Cull", CULL_BACK),
            ("_SrcBlend", SRC_ALPHA),
            ("_DstBlend", ONE_MINUS_SRC_ALPHA),
        ],
        &QUAD,
    );
    assert_pixels(
        &image,
        |x, y| {
            let src = shaded(x, y);
            let alpha = src[3] / 255.0;
            let back = backdrop();
            // The alpha factors stay One and Zero, so the alpha channel is the source alpha.
            std::array::from_fn(|c| {
                if c == 3 {
                    src[3]
                } else {
                    src[c] * alpha + back[c] * (1.0 - alpha)
                }
            })
        },
        BLEND_EXTRA_STEPS,
        "alpha blend",
    );
}

/// Both triangles of the quad, wound counterclockwise as displayed.
const BACK_FACING: [u16; 6] = [0, 2, 1, 0, 3, 2];

#[test]
fn a_back_facing_quad_vanishes_with_cull_back_and_appears_with_cull_off() {
    let blend_off = [("_SrcBlend", 1.0), ("_DstBlend", 0.0)];
    let culled = draw(
        [("_Cull", CULL_BACK), blend_off[0], blend_off[1]],
        &BACK_FACING,
    );
    assert_pixels(&culled, |_, _| backdrop(), 0.0, "cull back");
    let drawn = draw(
        [("_Cull", CULL_OFF), blend_off[0], blend_off[1]],
        &BACK_FACING,
    );
    assert_pixels(&drawn, shaded, 0.0, "cull off");
}
