//! A 32-bit float render texture read through a shader's filtering sampler.
//!
//! A device without `FLOAT32_FILTERABLE` cannot filter the texture, and wgpu refuses a pipeline
//! that samples a non-filterable float texture with a filtering sampler. The draw must still
//! build and read the nearest texel; a device that can filter it reads the linear blend.
//!
//! The shader's sampler is one no texture parameter names, as a shared inline sampler of a Unity
//! shader is, so nothing but the draw's own textures says what it reads.

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    BLIT, blit_parameters, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};
use worldrunner_unity_serialized::shader_parameters::ResourceKind;

use super::camera::fixture;
use super::rig::{BLIT_QUAD, Rig, blit_quad, layer, pixel, texture};
use crate::common::Gpu;

const R32G32B32A32_SFLOAT: u32 = 52;
const RGBA8: u32 = 8;
const LEFT_ONLY: u32 = 1 << 1;
const FLOAT_TEXTURE: TextureId = TextureId(10);
const OUTPUT: TextureId = TextureId(11);
const BLIT_SHADER: ShaderId = ShaderId(2);
const SIDE: u32 = 8;

/// What the camera draws into the left column of the 2x2 float texture, and the yellow it clears
/// the right column to.
const DRAWN: [f32; 4] = [0.2, 0.4, 0.6, 0.8];
const CLEARED: [f32; 4] = [1.0, 1.0, 0.0, 1.0];

/// The blit pass over a source whose sampler binding no texture parameter pairs with.
fn unpaired_blit() -> ProgramShader {
    let mut parameters = blit_parameters("_MainTex");
    for binding in &mut parameters.bindings {
        if let ResourceKind::Texture { sampler, .. } = &mut binding.kind {
            *sampler = None;
        }
    }
    let synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], BLIT, "fs_main", parameters)],
        }],
    ));
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// A camera draws the left column of a 2x2 float texture and clears the right one, and a blit
/// reads it with the shader's sampler into an 8x8 RGBA8 texture, whose bytes it returns.
fn blitted_up(rig: &mut Rig) -> Vec<u8> {
    let mut frame: OffscreenFrame = fixture(rig, LEFT_ONLY);
    frame.render_textures[0].graphics_format = R32G32B32A32_SFLOAT;
    frame.render_textures[0].size = [2, 2];
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties = layer(&[("_Tint", DRAWN)]);
    rig.state.add_shader(BLIT_SHADER, unpaired_blit());
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    frame.blit_mesh = Some(BLIT_QUAD);
    let mut output = texture(11, [SIDE, SIDE]);
    output.graphics_format = RGBA8;
    frame.render_textures.push(output);
    frame.materials.insert(
        MaterialId(3),
        Material {
            shader: Some(BLIT_SHADER),
            keywords: Vec::new(),
            properties: layer(&[("_Tint", [1.0; 4])]),
            fallback: PropertyLayer::default(),
            render_queue: 2000,
        },
    );
    frame.records.push(Record::Blit {
        source: FLOAT_TEXTURE,
        target: OUTPUT,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    let images = rig.run(&frame, &[(OUTPUT, 0, 0)]).unwrap();
    images.into_iter().next().unwrap()
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The texel of column `x` of the 8 across when the 2-texel texture is read at the nearest texel:
/// the drawn column for the left half and the cleared one for the right.
fn nearest(x: u32) -> [u8; 4] {
    let texel = if x < SIDE / 2 { DRAWN } else { CLEARED };
    texel.map(byte)
}

/// The texel of column `x` when the texture is read linearly with the edge clamped: the pixel
/// center sits at `(x + 0.5) / 8` of the width, the texel centers at 1/4 and 3/4.
fn linear(x: u32) -> [u8; 4] {
    let weight = (((x as f32 + 0.5) / SIDE as f32 - 0.25) / 0.5).clamp(0.0, 1.0);
    std::array::from_fn(|channel| byte(DRAWN[channel] * (1.0 - weight) + CLEARED[channel] * weight))
}

fn assert_row(image: &[u8], expected: impl Fn(u32) -> [u8; 4], tolerance: u8) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (got, want) = (pixel(image, SIDE, (x, y)), expected(x));
            assert!(
                got.iter()
                    .zip(want)
                    .all(|(&g, w)| g.abs_diff(w) <= tolerance),
                "({x}, {y}) is {got:?}, expected {want:?}"
            );
        }
    }
}

#[test]
fn a_float_texture_the_device_cannot_filter_is_read_at_the_nearest_texel_through_a_filtering_sampler()
 {
    // The default test device does not ask for FLOAT32_FILTERABLE.
    let mut rig = Rig::new();
    assert!(
        !rig.gpu
            .device
            .features()
            .contains(wgpu::Features::FLOAT32_FILTERABLE)
    );
    let image = blitted_up(&mut rig);
    assert_row(&image, nearest, 1);
}

#[test]
fn a_float_texture_is_filtered_when_the_device_has_float32_filterable() {
    let mut rig = Rig::on(Gpu::with_optional(wgpu::Features::FLOAT32_FILTERABLE));
    let filterable = rig
        .gpu
        .device
        .features()
        .contains(wgpu::Features::FLOAT32_FILTERABLE);
    eprintln!("FLOAT32_FILTERABLE on this adapter: {filterable}");
    let image = blitted_up(&mut rig);
    if filterable {
        assert_row(&image, linear, 3);
    } else {
        assert_row(&image, nearest, 1);
    }
}
