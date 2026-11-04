//! Render textures of the float formats the game asks for: a camera and a blit write them and
//! the readback holds the values the shaders produced.

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::shader::ProgramShader;
use worldrunner_render_program::synthetic::shaders::blit_passes;

use super::camera::fixture;
use super::rig::{BLIT_QUAD, BLUE, RED, Rig, blit_quad, image, layer, source};

const R16_SFLOAT: u32 = 45;
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

fn blit_setup(rig: &mut Rig, size: [u32; 2]) {
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
fn blit_frame(red: f32, format: u32, size: [u32; 2]) -> OffscreenFrame {
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
