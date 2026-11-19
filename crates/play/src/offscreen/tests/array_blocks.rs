//! Stored 2D arrays in block-compressed and four-byte formats as render-program's source arrays:
//! the blocks and texels go up as they are stored, every level of every layer, and what the host
//! cannot draw is refused by name.

use worldrunner_disunity_runtime::PlayerTexture;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::SourceDimension;

use super::super::assets::texture_of;
use super::assets::texture;

/// A stored array of `depth` layers of `side` by `side` texels with `mips` levels, in the block
/// format `format` (10 is DXT1 with 8 byte blocks, 12 is DXT5 with 16 byte blocks), holding bytes
/// that count up from zero.
fn block_array(format: u32, side: u32, (depth, mips): (u32, u32), linear: bool) -> PlayerTexture {
    let block = if format == 10 { 8 } else { 16 };
    let blocks_of = |level: u32| (side >> level).max(1).div_ceil(4).pow(2) as usize;
    let layer: usize = (0..mips).map(|level| blocks_of(level) * block).sum();
    PlayerTexture {
        width: side,
        height: side,
        mips,
        depth,
        bytes: (0..=u8::MAX).cycle().take(layer * depth as usize).collect(),
        ..texture(format, linear, Vec::new())
    }
}

#[test]
fn a_dxt1_or_dxt5_array_goes_up_as_its_blocks_with_every_level_in_the_colour_space_it_has() {
    // 8x8 texels with four levels: four blocks, then one block each for 4x4, 2x2 and 1x1.
    for (format, srgb, linear) in [(10, 96, 97), (12, 100, 101)] {
        for is_linear in [false, true] {
            let stored = block_array(format, 8, (3, 4), is_linear);
            let made = texture_of(TextureId(1), &stored, None, true).unwrap();
            assert_eq!(made.dimension, SourceDimension::D2Array(3), "{format}");
            assert_eq!((made.size, made.mips), ([8, 8], 4), "{format}");
            assert_eq!(
                made.graphics_format,
                if is_linear { linear } else { srgb },
                "{format} linear {is_linear}"
            );
            assert_eq!(made.bytes, stored.bytes, "the blocks are not changed");
        }
    }
}

#[test]
fn a_block_array_on_a_device_without_block_compression_is_refused_by_name() {
    use crate::textures::PlayerTextureError::ArrayNeedsBlocks;
    for (format, name) in [(10, "DXT1"), (12, "DXT5")] {
        let error = texture_of(
            TextureId(1),
            &block_array(format, 4, (2, 1), false),
            None,
            false,
        )
        .unwrap_err();
        assert_eq!(error, ArrayNeedsBlocks { format: name });
    }
    // The 2D counterpart is decoded on such a device.
    let plain = PlayerTexture {
        width: 4,
        height: 4,
        ..texture(10, false, vec![0; 8])
    };
    let decoded = texture_of(TextureId(1), &plain, None, false).unwrap();
    assert_eq!(
        (
            decoded.dimension,
            decoded.graphics_format,
            decoded.bytes.len()
        ),
        (SourceDimension::D2, 4, 4 * 4 * 4)
    );
}

#[test]
fn a_block_array_that_is_not_whole_blocks_or_whole_chains_is_an_error_by_name() {
    use crate::textures::PlayerTextureError::{ArrayBlockSize, Levels};
    let id = TextureId(1);
    let odd = PlayerTexture {
        width: 6,
        ..block_array(10, 4, (2, 1), false)
    };
    assert_eq!(
        texture_of(id, &odd, None, true).unwrap_err(),
        ArrayBlockSize {
            format: "DXT1",
            width: 6,
            height: 4
        }
    );
    // One layer of 4x4 DXT1 with three levels is 24 bytes; this one holds 8.
    let short = PlayerTexture {
        mips: 3,
        ..block_array(10, 4, (2, 1), false)
    };
    assert_eq!(
        texture_of(id, &short, None, true).unwrap_err(),
        Levels {
            mips: 3,
            needed: 24,
            actual: 8
        }
    );
}

#[test]
fn an_array_in_a_four_byte_graphics_format_with_no_texture_format_goes_up_as_it_is() {
    // 87 stores four bytes a texel, with every level of every layer, and the engine has no
    // TextureFormat for it. 2x2 texels and two levels: 16 + 4 bytes a layer.
    let stored = PlayerTexture {
        format: 0,
        graphics_format: 87,
        width: 2,
        height: 2,
        mips: 2,
        depth: 2,
        bytes: (0..40).collect(),
        ..texture(0, false, Vec::new())
    };
    let made = texture_of(TextureId(1), &stored, None, true).unwrap();
    assert_eq!(made.dimension, SourceDimension::D2Array(2));
    assert_eq!((made.size, made.mips, made.graphics_format), ([2, 2], 2, 4));
    assert_eq!(made.bytes, stored.bytes);
    // The same holds without block compression, which 87 does not use.
    assert_eq!(
        texture_of(TextureId(1), &stored, None, false)
            .unwrap()
            .bytes,
        stored.bytes
    );
}
