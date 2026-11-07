//! A source texture kept in a block-compressed format: the player stores some of its terrain
//! maps as unsigned-float BC6H, and the draw samples the decoded texels.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::SourceTexture;

use super::formats::{blit_frame, blit_setup};
use super::rig::Rig;

/// `GraphicsFormat` of unsigned-float BC6H.
const BC6H_UFLOAT: u32 = 106;
const R32G32B32A32_SFLOAT: u32 = 52;
/// The largest finite half float, which a BC6H endpoint of all ones decodes to.
const HALF_MAX: f32 = 65504.0;

/// One BC6H block of the mode with a single subset and ten-bit endpoints: every endpoint holds
/// `endpoint` and every index is zero, so every texel of the block is the first endpoint.
fn flat_block(endpoint: u16) -> [u8; 16] {
    let mut bits = 3_u128;
    for slot in 0..6 {
        bits |= u128::from(endpoint) << (5 + slot * 10);
    }
    bits.to_le_bytes()
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks(4)
        .map(|float| f32::from_le_bytes(float.try_into().unwrap()))
        .collect()
}

/// An 8x4 source of two blocks: the left holds zeros and the right the largest endpoint.
fn two_blocks() -> SourceTexture {
    let bytes = [flat_block(0), flat_block(1023)].concat();
    SourceTexture {
        id: TextureId(5),
        size: [8, 4],
        graphics_format: BC6H_UFLOAT,
        bytes,
        sampler: None,
    }
}

#[test]
fn a_bc6h_source_is_sampled_as_the_unsigned_floats_its_blocks_hold() {
    let mut rig = Rig::new();
    assert!(
        rig.gpu
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        "the adapter must offer block compression for this test"
    );
    blit_setup(&mut rig, [4, 4]);
    rig.state
        .set_texture(&mut rig.runtime, &two_blocks())
        .unwrap();
    let frame = blit_frame(1.0, R32G32B32A32_SFLOAT, [8, 4]);
    let image = rig.run(&frame, &[(TextureId(11), 0, 0)]).unwrap().remove(0);
    let texels = floats(&image);
    assert_eq!(texels.len(), 8 * 4 * 4, "four floats for each of 32 texels");
    for y in 0..4 {
        for x in 0..8 {
            let at = (y * 8 + x) * 4;
            // The blit multiplies the source by the tint, whose red is 1 and whose other
            // channels are 1; a BC6H texel has no alpha, so it reads as 1.
            let expected = if x < 4 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [HALF_MAX, HALF_MAX, HALF_MAX, 1.0]
            };
            assert_eq!(texels[at..at + 4], expected, "texel ({x}, {y})");
        }
    }
}

#[test]
fn a_bc6h_source_whose_bytes_are_not_whole_blocks_is_a_named_error() {
    let mut rig = Rig::new();
    let mut short = two_blocks();
    short.bytes.truncate(31);
    let error = rig.state.set_texture(&mut rig.runtime, &short).unwrap_err();
    assert!(
        matches!(error, OffscreenError::Program(_)),
        "a short upload is refused by the runtime: {error}"
    );
}
