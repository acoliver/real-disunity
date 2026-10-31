#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    graphics::{MipLimit, StreamingInfo, Texture2DArray, TextureSettings},
    write::ObjectWriter,
};

/// An array of three 4x2 slices with two mip levels, laid out as the player writes them.
fn array_bytes(big_endian: bool, inline: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Atlas").unwrap().align();
    w.i32(1).u8(1).u8(0).align();
    w.i32(96).i32(4).i32(2).i32(3).i32(2).i32(5).u32(36);
    w.i32(2).i32(8).f32(0.5).i32(0).i32(1).i32(2);
    w.i32(7).u8(1).u8(1).u8(1).align();
    w.string("Terrain").unwrap().align();
    if inline {
        w.byte_array(&[9, 8, 7, 6, 5]).unwrap();
        w.u64(0).u32(0).string("").unwrap().align();
    } else {
        w.byte_array(&[]).unwrap();
        w.u64(4_096)
            .u32(36)
            .string("sharedassets0.assets.resS")
            .unwrap();
        w.align();
    }
    w.finish()
}

fn expected(inline: bool) -> Texture2DArray {
    let stream = if inline {
        StreamingInfo {
            offset: 0,
            size: 0,
            path: String::new(),
        }
    } else {
        StreamingInfo {
            offset: 4_096,
            size: 36,
            path: "sharedassets0.assets.resS".into(),
        }
    };
    Texture2DArray {
        name: "Atlas".into(),
        color_space: 1,
        is_alpha_channel_optional: true,
        format: 96,
        width: 4,
        height: 2,
        depth: 3,
        mip_count: 2,
        stripped_mips: 5,
        data_size: 36,
        settings: TextureSettings {
            filter_mode: 2,
            aniso: 8,
            mip_bias: 0.5,
            wrap: [0, 1, 2],
        },
        lightmap_format: 7,
        is_readable: true,
        is_pre_processed: true,
        mip_limit: MipLimit {
            ignore: true,
            group: "Terrain".into(),
        },
        image_data: if inline { 96..101 } else { 96..96 },
        stream,
    }
}

#[test]
fn decodes_every_field_in_both_byte_orders() {
    for big_endian in [false, true] {
        for inline in [false, true] {
            let bytes = array_bytes(big_endian, inline);
            let decoded: Texture2DArray =
                decode_one(ClassId::TEXTURE_2D_ARRAY, bytes.clone(), big_endian).unwrap();
            assert_eq!(decoded, expected(inline));
            let image = &bytes[decoded.image_data];
            assert_eq!(image, if inline { &[9, 8, 7, 6, 5][..] } else { &[] });
        }
    }
}

#[test]
fn every_truncated_array_fails() {
    let bytes = array_bytes(false, true);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Texture2DArray>(ClassId::TEXTURE_2D_ARRAY, bytes[..end].to_vec(), false)
                .unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut bytes = array_bytes(false, true);
    bytes.extend_from_slice(&[0; 4]);
    let result = decode_one::<Texture2DArray>(ClassId::TEXTURE_2D_ARRAY, bytes, false);
    assert!(result.is_err());
}
