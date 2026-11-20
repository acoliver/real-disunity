//! The layouts of cubes and volumes, laid out as the player writes them.
#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    graphics::{Cubemap, StreamingInfo, Texture3D, TextureSettings},
    object::PPtr,
    write::ObjectWriter,
};

fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

/// A cube of 4x4 faces with three mip levels in `TextureFormat` 24, whose pixels are streamed or
/// inline, made from two textures.
fn cube_bytes(big_endian: bool, inline: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Sky").unwrap().u8(1).align();
    w.i32(4).i32(4).u32(36).i32(0).i32(24).i32(3);
    w.u8(0)
        .u8(1)
        .u8(0)
        .align()
        .string("Probes")
        .unwrap()
        .u8(0)
        .align()
        .i32(0);
    w.i32(6).i32(2).i32(1).i32(1).f32(0.0).i32(1).i32(1).i32(1);
    w.i32(0).i32(0).byte_array(&[]).unwrap();
    if inline {
        w.byte_array(&[9, 8, 7])
            .unwrap()
            .u64(0)
            .u32(0)
            .string("")
            .unwrap();
    } else {
        w.byte_array(&[])
            .unwrap()
            .u64(4_096)
            .u32(216)
            .string("sharedassets0.assets.resS")
            .unwrap();
    }
    w.align();
    w.i32(2).pptr(ptr(5)).pptr(ptr(6));
    w.finish()
}

#[test]
fn a_cube_has_the_fields_of_a_2d_texture_and_the_textures_it_was_made_from() {
    for big_endian in [false, true] {
        for inline in [false, true] {
            let cube: Cubemap =
                decode_one(ClassId::CUBEMAP, cube_bytes(big_endian, inline), big_endian).unwrap();
            let texture = &cube.texture;
            assert_eq!(
                (texture.name.as_str(), texture.width, texture.height),
                ("Sky", 4, 4)
            );
            assert_eq!(
                (texture.format, texture.mipmaps.count, texture.image_count),
                (24, 3, 6)
            );
            assert_eq!(texture.complete_image_size, 36);
            assert_eq!(
                texture.settings,
                TextureSettings {
                    filter_mode: 1,
                    aniso: 1,
                    mip_bias: 0.0,
                    wrap: [1, 1, 1]
                }
            );
            let stream = if inline {
                StreamingInfo {
                    offset: 0,
                    size: 0,
                    path: String::new(),
                }
            } else {
                StreamingInfo {
                    offset: 4_096,
                    size: 216,
                    path: "sharedassets0.assets.resS".into(),
                }
            };
            assert_eq!(texture.stream, stream);
            assert_eq!(texture.image_data.len(), if inline { 3 } else { 0 });
            assert_eq!(cube.source_textures, [ptr(5), ptr(6)]);
        }
    }
}

#[test]
fn every_truncated_cube_fails_and_trailing_bytes_are_rejected() {
    let bytes = cube_bytes(false, true);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Cubemap>(ClassId::CUBEMAP, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    let mut longer = bytes;
    longer.extend_from_slice(&[0; 4]);
    assert!(decode_one::<Cubemap>(ClassId::CUBEMAP, longer, false).is_err());
}

/// A volume of 4x2x3 voxels in `GraphicsFormat` 88 with two levels, laid out as the player writes
/// a 3D texture: no stripped-mip count and no mip limit.
fn volume_bytes(big_endian: bool, inline: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Noise").unwrap().align();
    w.i32(0).u8(1).align();
    w.i32(88).i32(4).i32(2).i32(3).i32(2).u32(116);
    w.i32(2).i32(0).f32(0.5).i32(2).i32(2).i32(2);
    w.i32(0).u8(1).align();
    if inline {
        w.byte_array(&[1, 2, 3, 4, 5])
            .unwrap()
            .u64(0)
            .u32(0)
            .string("")
            .unwrap();
    } else {
        w.byte_array(&[])
            .unwrap()
            .u64(512)
            .u32(116)
            .string("resources.assets.resS")
            .unwrap();
    }
    w.align();
    w.finish()
}

fn expected_volume(inline: bool) -> Texture3D {
    Texture3D {
        name: "Noise".into(),
        color_space: 0,
        is_alpha_channel_optional: true,
        format: 88,
        width: 4,
        height: 2,
        depth: 3,
        mip_count: 2,
        data_size: 116,
        settings: TextureSettings {
            filter_mode: 2,
            aniso: 0,
            mip_bias: 0.5,
            wrap: [2, 2, 2],
        },
        lightmap_format: 0,
        is_readable: true,
        image_data: if inline { 80..85 } else { 80..80 },
        stream: if inline {
            StreamingInfo {
                offset: 0,
                size: 0,
                path: String::new(),
            }
        } else {
            StreamingInfo {
                offset: 512,
                size: 116,
                path: "resources.assets.resS".into(),
            }
        },
    }
}

#[test]
fn a_volume_decodes_every_field_in_both_byte_orders() {
    for big_endian in [false, true] {
        for inline in [false, true] {
            let bytes = volume_bytes(big_endian, inline);
            let decoded: Texture3D =
                decode_one(ClassId::TEXTURE_3D, bytes.clone(), big_endian).unwrap();
            assert_eq!(decoded, expected_volume(inline));
            let image = &bytes[decoded.image_data];
            assert_eq!(image, if inline { &[1, 2, 3, 4, 5][..] } else { &[] });
        }
    }
}

#[test]
fn every_truncated_volume_fails_and_trailing_bytes_are_rejected() {
    let bytes = volume_bytes(false, true);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Texture3D>(ClassId::TEXTURE_3D, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    let mut longer = bytes;
    longer.extend_from_slice(&[0; 4]);
    assert!(decode_one::<Texture3D>(ClassId::TEXTURE_3D, longer, false).is_err());
}
