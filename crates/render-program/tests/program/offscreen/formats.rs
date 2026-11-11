//! Render textures of the float formats the game asks for: a camera and a blit write them and
//! the readback holds the values the shaders produced.

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

use super::camera::fixture;
use super::rig::{BLIT_QUAD, BLUE, RED, Rig, blit_quad, image, layer, pixel, source};

const R16_SFLOAT: u32 = 45;
const R32G32B32A32_SFLOAT: u32 = 52;
const TARGET: TextureId = TextureId(10);
const BLIT_SHADER: ShaderId = ShaderId(2);
const BLIT_TARGET: TextureId = TextureId(11);
const LEFT_ONLY: u32 = 1 << 1;

/// The half-float bits of 0.375 (exponent -2, mantissa .5) and of 1.0.
const HALF_0_375: u16 = 0x3600;
const HALF_1: u16 = 0x3C00;

fn halves(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks(2)
        .map(|half| u16::from_le_bytes([half[0], half[1]]))
        .collect()
}

#[test]
fn a_camera_draws_into_an_r16_float_render_texture() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, LEFT_ONLY);
    frame.render_textures[0].graphics_format = R16_SFLOAT;
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties =
        layer(&[("_Tint", [0.375, 0.0, 0.0, 1.0])]);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_eq!(image.len(), 8 * 8 * 2, "one half float for each texel");
    let texels = halves(&image);
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(texels[y * 8 + x], HALF_0_375, "drawn ({x}, {y})");
            assert_eq!(texels[y * 8 + x + 4], HALF_1, "cleared ({}, {y})", x + 4);
        }
    }
}

#[test]
fn a_blit_writes_the_red_channel_of_its_result_into_an_r16_float_render_texture() {
    let mut rig = Rig::new();
    blit_setup(&mut rig, [4, 4]);
    let frame = blit_frame(0.375, R16_SFLOAT, [4, 4]);
    let image = rig.run(&frame, &[(BLIT_TARGET, 0, 0)]).unwrap().remove(0);
    let texels = halves(&image);
    for y in 0..4 {
        // The source is red on the left and blue on the right: the tint's 0.375 times 1 and 0.
        assert_eq!(texels[y * 4], HALF_0_375, "left ({y})");
        assert_eq!(texels[y * 4 + 1], HALF_0_375);
        assert_eq!(texels[y * 4 + 2], 0, "right ({y})");
        assert_eq!(texels[y * 4 + 3], 0);
    }
}

pub fn blit_setup(rig: &mut Rig, size: [u32; 2]) {
    let synthetic = blit_passes("_MainTex");
    rig.state.add_shader(
        BLIT_SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    let halves = image(size, |x, _| if x < size[0] / 2 { RED } else { BLUE });
    rig.state
        .set_texture(&mut rig.runtime, &source(5, size, halves))
        .unwrap();
}

/// A blit of source 5 through pass 0, the source times `red`, into a render texture of `format`.
pub fn blit_frame(red: f32, format: u32, size: [u32; 2]) -> OffscreenFrame {
    let mut target = super::rig::texture(11, size);
    target.graphics_format = format;
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![target],
        materials: std::collections::BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(BLIT_SHADER),
                keywords: Vec::new(),
                properties: layer(&[("_Tint", [red, 1.0, 1.0, 1.0])]),
                fallback: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: TextureId(5),
            target: BLIT_TARGET,
            material: Some(MaterialId(1)),
            pass: 0,
        }],
        ..OffscreenFrame::default()
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

const DRAWN: [f32; 4] = [0.2, 0.4, 0.6, 0.8];
const CLEARED: [f32; 4] = [1.0, 1.0, 0.0, 1.0];

#[test]
fn a_camera_draws_into_an_r32g32b32a32_float_render_texture() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, LEFT_ONLY);
    frame.render_textures[0].graphics_format = R32G32B32A32_SFLOAT;
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties = layer(&[("_Tint", DRAWN)]);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_eq!(image.len(), 8 * 8 * 16, "four floats for each texel");
    let texels = floats(&image);
    for y in 0..8 {
        for x in 0..4 {
            let at = (y * 8 + x) * 4;
            assert_eq!(texels[at..at + 4], DRAWN, "drawn ({x}, {y})");
            let at = (y * 8 + x + 4) * 4;
            assert_eq!(texels[at..at + 4], CLEARED, "cleared ({}, {y})", x + 4);
        }
    }
}

/// A 2x2 float render texture whose left column a camera draws and whose right column it clears,
/// and a blit of it through the blit pass's filtering sampler into an 8x8 RGBA8 texture.
fn float_texture_blitted_up(rig: &mut Rig, format: u32) -> Vec<u8> {
    let mut frame = fixture(rig, LEFT_ONLY);
    frame.render_textures[0].graphics_format = format;
    frame.render_textures[0].size = [2, 2];
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties = layer(&[("_Tint", DRAWN)]);
    blit_setup(rig, [4, 4]);
    frame.blit_mesh = Some(BLIT_QUAD);
    frame.render_textures.push(super::rig::texture(11, [8, 8]));
    frame.materials.insert(
        MaterialId(3),
        blit_frame(1.0, 8, [8, 8])
            .materials
            .remove(&MaterialId(1))
            .unwrap(),
    );
    frame.records.push(Record::Blit {
        source: TARGET,
        target: BLIT_TARGET,
        material: Some(MaterialId(3)),
        pass: 0,
    });
    rig.run(&frame, &[(BLIT_TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn a_float_render_texture_the_device_cannot_filter_is_sampled_at_the_nearest_texel() {
    let mut rig = Rig::new();
    let image = float_texture_blitted_up(&mut rig, R32G32B32A32_SFLOAT);
    // Predicted from nearest sampling: every texel of the left four columns reads the drawn
    // column of the 2x2 texture and the right four read the cleared one. A linear filter would
    // blend them at column 3 and column 4. 0.2, 0.4, 0.6 and 0.8 of 255 are 51, 102, 153 and 204.
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(
                pixel(&image, 8, (x, y)),
                [51, 102, 153, 204],
                "left ({x}, {y})"
            );
            assert_eq!(
                pixel(&image, 8, (x + 4, y)),
                [255, 255, 0, 255],
                "right ({x}, {y})"
            );
        }
    }
}

#[test]
fn a_float_render_texture_the_device_can_filter_is_still_filtered() {
    const R16G16B16A16_SFLOAT: u32 = 48;
    let mut rig = Rig::new();
    let image = float_texture_blitted_up(&mut rig, R16G16B16A16_SFLOAT);
    // Column 3 of 8 reads u = 0.4375, which is 0.375 of the way from the drawn texel to the
    // cleared one: green 0.4 * 0.625 + 1.0 * 0.375 = 0.625 (159) and blue 0.6 * 0.625 = 0.375 (96).
    let texel = pixel(&image, 8, (3, 4));
    assert_eq!((texel[1], texel[2]), (159, 96), "{texel:?}");
    // Column 0 is clamped to the drawn texel.
    assert_eq!(pixel(&image, 8, (0, 4)), [51, 102, 153, 204]);
}
