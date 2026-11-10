//! A blit with no material between textures that differ in size or format: the source is drawn
//! over the whole target through the built-in sampling program, so the target holds what the
//! source holds, decoded and encoded by the formats.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    OffscreenFrame, Record, RenderTextureSpec, SourceTexture,
};

use super::rig::{BLUE, RED, Rig, image, pixel, texture};

/// `GraphicsFormat` of unsigned-normalised RGBA8 in sRGB and of sRGB BC3.
const RGBA8_SRGB: u32 = 4;
const BC3_SRGB: u32 = 100;
const SOURCE: TextureId = TextureId(5);
const TARGET: TextureId = TextureId(11);

fn srgb_target(size: [u32; 2]) -> RenderTextureSpec {
    RenderTextureSpec {
        graphics_format: RGBA8_SRGB,
        srgb: true,
        ..texture(11, size)
    }
}

fn copy_into(target: RenderTextureSpec) -> OffscreenFrame {
    OffscreenFrame {
        render_textures: vec![target],
        records: vec![Record::Blit {
            source: SOURCE,
            target: TARGET,
            material: None,
            pass: 0,
        }],
        ..OffscreenFrame::default()
    }
}

fn upload(rig: &mut Rig, source: &SourceTexture) {
    rig.state.set_texture(&mut rig.runtime, source).unwrap();
}

fn texels(size: [u32; 2], graphics_format: u32, bytes: Vec<u8>) -> SourceTexture {
    SourceTexture {
        id: SOURCE,
        size,
        graphics_format,
        bytes,
        sampler: None,
    }
}

fn halves(size: [u32; 2]) -> Vec<u8> {
    image(size, |x, _| if x < size[0] / 2 { RED } else { BLUE })
}

/// One BC3 block of a single color: both alpha endpoints 255 and both color endpoints `rgb565`,
/// with every index zero, so every texel is that color. Red is 0xF800 and blue 0x001F.
fn flat_bc3(rgb565: u16) -> [u8; 16] {
    let mut block = [0_u8; 16];
    block[..2].fill(255);
    block[8..10].copy_from_slice(&rgb565.to_le_bytes());
    block[10..12].copy_from_slice(&rgb565.to_le_bytes());
    block
}

#[test]
fn a_block_compressed_srgb_source_is_copied_into_an_srgb_render_texture() {
    let mut rig = Rig::new();
    let blocks = [flat_bc3(0xF800), flat_bc3(0x001F)].concat();
    upload(&mut rig, &texels([8, 4], BC3_SRGB, blocks));
    let frame = copy_into(srgb_target([8, 4]));
    let result = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    // Channel values of 0 and 255 survive the decode and the encode exactly.
    for y in 0..4 {
        for x in 0..8 {
            let expected = if x < 4 { RED } else { BLUE };
            assert_eq!(pixel(&result, 8, (x, y)), expected, "texel ({x}, {y})");
        }
    }
}

#[test]
fn a_blit_without_a_material_scales_the_source_to_the_size_of_the_target() {
    let mut rig = Rig::new();
    upload(&mut rig, &texels([4, 4], RGBA8_SRGB, halves([4, 4])));
    let down = rig
        .run(&copy_into(srgb_target([2, 2])), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    // Each target texel center falls between two source texels of one color.
    assert_eq!(pixel(&down, 2, (0, 0)), RED);
    assert_eq!(pixel(&down, 2, (1, 1)), BLUE);

    let mut point = texels([2, 2], RGBA8_SRGB, halves([2, 2]));
    point.sampler = Some(SamplerState {
        wrap_u: Wrap::Clamp,
        wrap_v: Wrap::Clamp,
        filter: Filter::Point,
    });
    upload(&mut rig, &point);
    let up = rig
        .run(&copy_into(srgb_target([4, 4])), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    // The source's own point filter keeps the edge between the halves sharp.
    for y in 0..4 {
        assert_eq!(pixel(&up, 4, (1, y)), RED, "left half ({y})");
        assert_eq!(pixel(&up, 4, (2, y)), BLUE, "right half ({y})");
    }
}

#[test]
fn srgb_values_of_a_source_reach_a_target_of_another_size_unchanged() {
    let mut rig = Rig::new();
    let tone = [188, 100, 50, 200];
    let bytes = image([4, 4], |_, _| tone);
    upload(&mut rig, &texels([4, 4], RGBA8_SRGB, bytes));
    let result = rig
        .run(&copy_into(srgb_target([2, 2])), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        // Sampling decodes to linear and the target encodes back; a device rounds that trip to
        // within one level of the stored byte.
        let found = pixel(&result, 2, (x, y));
        for channel in 0..4 {
            assert!(
                found[channel].abs_diff(tone[channel]) <= 1,
                "texel ({x}, {y}) channel {channel}: {found:?} against {tone:?}"
            );
        }
    }
}

#[test]
fn a_source_the_device_cannot_filter_stays_a_named_error() {
    let mut rig = Rig::new();
    let depth = RenderTextureSpec {
        graphics_format: 0,
        depth_bits: 16,
        ..texture(5, [4, 4])
    };
    let mut frame = copy_into(srgb_target([4, 4]));
    frame.render_textures.push(depth);
    let error = rig.build(&frame).err().unwrap();
    assert!(
        matches!(&error, OffscreenError::BlitNeedsMaterial { reason } if reason.contains("Depth16Unorm")),
        "{error}"
    );
}
