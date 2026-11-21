//! Stored cubes and 3D textures as render-program's source cubes and volumes: the faces, slices
//! and levels go up in the order the engine stored them, and what the host cannot draw is refused
//! by name.

use worldrunner_disunity_runtime::{PlayerTexture, TextureDimension};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::SourceDimension;

use super::super::assets::texture_of;
use super::assets::texture;
use crate::textures::PlayerTextureError;

/// A stored cube of `side` by `side` texels with `mips` levels in the `TextureFormat` `format`.
fn cube(format: u32, side: u32, mips: u32, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        width: side,
        height: side,
        mips,
        depth: 6,
        dimension: TextureDimension::Cube,
        graphics_format: 8,
        ..texture(format, false, bytes)
    }
}

/// A stored volume of `width` by `height` texels and `depth` slices with `mips` levels.
fn volume(
    format: u32,
    (width, height, depth): (u32, u32, u32),
    mips: u32,
    bytes: Vec<u8>,
) -> PlayerTexture {
    PlayerTexture {
        width,
        height,
        mips,
        depth,
        dimension: TextureDimension::D3,
        graphics_format: 8,
        ..texture(format, false, bytes)
    }
}

#[test]
fn a_stored_cube_is_one_source_cube_with_every_level_of_each_face() {
    // Six faces of 2x2 RGBA32 texels, each followed by its 1x1 second level.
    let face = |texel: u8| [vec![texel; 16], vec![texel + 100; 4]].concat();
    let bytes: Vec<u8> = (1..=6).flat_map(face).collect();
    let made = texture_of(TextureId(3), &cube(4, 2, 2, bytes.clone()), None, false).unwrap();
    assert_eq!(
        (made.id, made.dimension, made.size, made.mips),
        (TextureId(3), SourceDimension::Cube, [2, 2], 2)
    );
    assert_eq!(made.graphics_format, 4, "RGBA8 in sRGB");
    assert_eq!(made.bytes, bytes);
}

#[test]
fn a_cube_of_a_narrower_format_is_read_as_rgba8_face_by_face() {
    // Six 1x1 RGB24 faces.
    let bytes: Vec<u8> = (0..6)
        .flat_map(|face| [face, face + 10, face + 20])
        .collect();
    let made = texture_of(TextureId(3), &cube(3, 1, 1, bytes), None, false).unwrap();
    let expected: Vec<u8> = (0..6)
        .flat_map(|face| [face, face + 10, face + 20, 255])
        .collect();
    assert_eq!(made.dimension, SourceDimension::Cube);
    assert_eq!(made.bytes, expected);
}

#[test]
fn argb32_and_bgra32_cubes_are_read_as_rgba8_in_the_order_their_bytes_name() {
    // Six 1x1 faces whose bytes are 1, 2, 3 and 4 in the order the format stores them.
    let bytes: Vec<u8> = (0..6).flat_map(|_| [1, 2, 3, 4]).collect();
    let argb = texture_of(TextureId(3), &cube(5, 1, 1, bytes.clone()), None, false).unwrap();
    assert_eq!(
        argb.bytes,
        (0..6).flat_map(|_| [2, 3, 4, 1]).collect::<Vec<u8>>()
    );
    let bgra = texture_of(TextureId(3), &cube(14, 1, 1, bytes), None, false).unwrap();
    assert_eq!(
        bgra.bytes,
        (0..6).flat_map(|_| [3, 2, 1, 4]).collect::<Vec<u8>>()
    );
}

#[test]
fn a_bc6h_cube_goes_up_as_unsigned_float_blocks_with_every_level_of_each_face() {
    // Six faces of 8x8 texels with a chain of 8x8, 4x4, 2x2 and 1x1: 4 + 1 + 1 + 1 blocks of 16
    // bytes, so 112 bytes a face.
    let bytes: Vec<u8> = (0..=u8::MAX).cycle().take(6 * 112).collect();
    let stored = cube(24, 8, 4, bytes.clone());
    let made = texture_of(TextureId(3), &stored, None, true).unwrap();
    assert_eq!(
        (made.dimension, made.graphics_format, made.mips),
        (SourceDimension::Cube, 106, 4)
    );
    assert_eq!(made.bytes, bytes);
    assert_eq!(
        texture_of(TextureId(3), &stored, None, false).unwrap_err(),
        PlayerTextureError::ArrayNeedsBlocks { format: "BC6H" }
    );
}

#[test]
fn a_dxt_cube_goes_up_as_blocks_on_a_device_that_samples_them_and_is_refused_otherwise() {
    // Six 4x4 DXT1 faces of one 8-byte block each.
    let stored = cube(10, 4, 1, (0..48).collect());
    let made = texture_of(TextureId(3), &stored, None, true).unwrap();
    assert_eq!((made.graphics_format, made.bytes), (96, (0..48).collect()));
    let error = texture_of(TextureId(3), &stored, None, false).unwrap_err();
    assert_eq!(
        error,
        PlayerTextureError::ArrayNeedsBlocks { format: "DXT1" }
    );
}

#[test]
fn a_cube_that_is_not_six_square_faces_of_whole_levels_is_refused_by_name() {
    let five = PlayerTexture {
        depth: 5,
        ..cube(4, 1, 1, vec![0; 20])
    };
    assert_eq!(
        texture_of(TextureId(3), &five, None, false).unwrap_err(),
        PlayerTextureError::CubeFaces {
            depth: 5,
            width: 1,
            height: 1
        }
    );
    let wide = PlayerTexture {
        width: 2,
        ..cube(4, 1, 1, vec![0; 48])
    };
    assert!(matches!(
        texture_of(TextureId(3), &wide, None, false),
        Err(PlayerTextureError::CubeFaces { .. })
    ));
    let uneven = cube(4, 1, 1, vec![0; 25]);
    assert!(matches!(
        texture_of(TextureId(3), &uneven, None, false),
        Err(PlayerTextureError::Layers {
            depth: 6,
            actual: 25
        })
    ));
}

#[test]
fn a_cube_in_a_format_the_host_does_not_read_is_unsupported_by_name() {
    // BC7 cube faces.
    let stored = cube(25, 4, 1, vec![0; 96]);
    let error = texture_of(TextureId(3), &stored, None, true).unwrap_err();
    assert_eq!(
        error,
        PlayerTextureError::Unsupported {
            format: "BC7 cube".into()
        }
    );
}

#[test]
fn a_stored_volume_is_one_source_volume_with_every_level_and_its_slices() {
    // 2x1x4 RGBA32 texels, then 1x1x2 and 1x1x1: 32, 8 and 4 bytes.
    let bytes: Vec<u8> = (0..44).collect();
    let stored = volume(4, (2, 1, 4), 3, bytes.clone());
    let made = texture_of(TextureId(5), &stored, None, false).unwrap();
    assert_eq!(
        (made.id, made.dimension, made.size, made.mips),
        (TextureId(5), SourceDimension::D3(4), [2, 1], 3)
    );
    assert_eq!(made.graphics_format, 4, "RGBA8 in sRGB");
    assert_eq!(made.bytes, bytes);
}

#[test]
fn a_volume_of_a_narrower_format_is_read_as_rgba8_level_by_level() {
    // 1x1x2 Alpha8 texels, then the 1x1x1 second level.
    let stored = volume(1, (1, 1, 2), 2, vec![10, 20, 30]);
    let made = texture_of(TextureId(5), &stored, None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::D3(2));
    assert_eq!(
        made.bytes,
        [255, 255, 255, 10, 255, 255, 255, 20, 255, 255, 255, 30]
    );
}

#[test]
fn a_volume_in_the_four_byte_data_format_goes_up_as_rgba8_in_the_colour_space_it_states() {
    // As a four-byte data volume is stored: no TextureFormat, GraphicsFormat 88 and four bytes a
    // texel, here 2x1x2 texels and a 1x1x1 second level.
    let stored = PlayerTexture {
        graphics_format: 88,
        linear: true,
        ..volume(0, (2, 1, 2), 2, (1..=20).collect())
    };
    let made = texture_of(TextureId(5), &stored, None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::D3(2));
    assert_eq!((made.graphics_format, made.bytes), (8, (1..=20).collect()));
    let srgb = PlayerTexture {
        linear: false,
        ..stored
    };
    assert_eq!(
        texture_of(TextureId(5), &srgb, None, false)
            .unwrap()
            .graphics_format,
        4
    );
}

#[test]
fn a_cube_in_graphics_format_87_is_stored_blue_first_and_goes_up_red_first() {
    // Six 1x1 faces of four bytes: texel k holds the bytes 4k..4k+3 as blue, green, red and alpha,
    // so it goes up as 4k+2, 4k+1, 4k and 4k+3.
    let stored = PlayerTexture {
        format: 0,
        graphics_format: 87,
        ..cube(0, 1, 1, (0..24).collect())
    };
    let made = texture_of(TextureId(3), &stored, None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::Cube);
    assert_eq!(made.graphics_format, 4, "RGBA8 in sRGB");
    let red_first: Vec<u8> = (0..6_u8)
        .flat_map(|k| [4 * k + 2, 4 * k + 1, 4 * k, 4 * k + 3])
        .collect();
    assert_eq!(made.bytes, red_first);
}

#[test]
fn a_volume_in_graphics_format_87_is_stored_blue_first_and_goes_up_red_first() {
    // 2x1x2 texels and a 1x1x1 second level, five texels of four bytes: texel k holds the bytes
    // 4k..4k+3 as blue, green, red and alpha, so it goes up as 4k+2, 4k+1, 4k and 4k+3.
    let stored = PlayerTexture {
        format: 0,
        graphics_format: 87,
        ..volume(0, (2, 1, 2), 2, (0..20).collect())
    };
    let made = texture_of(TextureId(5), &stored, None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::D3(2));
    assert_eq!(made.graphics_format, 4, "RGBA8 in sRGB");
    let red_first: Vec<u8> = (0..5_u8)
        .flat_map(|k| [4 * k + 2, 4 * k + 1, 4 * k, 4 * k + 3])
        .collect();
    assert_eq!(made.bytes, red_first);
}

#[test]
fn a_four_byte_data_volume_keeps_its_bytes_in_the_order_they_are_stored() {
    // GraphicsFormat 88 stores red, green, blue and alpha, and is the one four-byte format that
    // is not exchanged.
    let stored = PlayerTexture {
        graphics_format: 88,
        ..volume(0, (1, 1, 2), 1, (0..8).collect())
    };
    let made = texture_of(TextureId(5), &stored, None, false).unwrap();
    assert_eq!(made.bytes, (0..8).collect::<Vec<u8>>());
}

#[test]
fn a_plain_2d_texture_in_graphics_format_87_is_refused_because_it_has_no_texture_format() {
    // A plain texture is read through its TextureFormat, and 87 has none, so no 2D texture holds
    // it: only arrays, cubes and volumes do, and each of those exchanges its bytes.
    let stored = PlayerTexture {
        format: 0,
        graphics_format: 87,
        width: 1,
        height: 1,
        depth: 1,
        dimension: TextureDimension::D2,
        ..texture(0, false, vec![0, 1, 2, 3])
    };
    assert_eq!(
        texture_of(TextureId(2), &stored, None, false).unwrap_err(),
        PlayerTextureError::Unsupported {
            format: "TextureFormat 0".into()
        }
    );
}

#[test]
fn a_volume_whose_bytes_are_not_its_chain_or_whose_format_is_not_texels_is_refused_by_name() {
    let short = volume(4, (2, 1, 4), 3, vec![0; 43]);
    assert_eq!(
        texture_of(TextureId(5), &short, None, false).unwrap_err(),
        PlayerTextureError::Levels {
            mips: 3,
            needed: 44,
            actual: 43
        }
    );
    let too_many = volume(4, (1, 1, 2), 3, vec![0; 20]);
    assert_eq!(
        texture_of(TextureId(5), &too_many, None, false).unwrap_err(),
        PlayerTextureError::VolumeMips {
            mips: 3,
            size: [1, 1, 2]
        }
    );
    let blocks = volume(10, (4, 4, 2), 1, vec![0; 16]);
    assert_eq!(
        texture_of(TextureId(5), &blocks, None, true).unwrap_err(),
        PlayerTextureError::Unsupported {
            format: "DXT1 3D".into()
        }
    );
}
