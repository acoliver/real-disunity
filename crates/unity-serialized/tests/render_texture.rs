#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError, render_texture::RenderTexture, write::ObjectWriter,
};

/// The layout of Old World's `TimelineRenderTexture`, 100 bytes in the player file.
fn render_texture_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("TimelineRenderTexture").unwrap().align();
    w.i32(0).i32(1024).i32(1024).i32(1).i32(-1);
    w.i32(90).i32(8);
    w.bytes(&[0, 1, 0, 0, 0, 0, 1, 0]);
    w.i32(1).i32(0).f32(0.0).i32(1).i32(1).i32(1);
    w.i32(2).i32(1).i32(2);
    w.finish()
}

fn expected() -> RenderTexture {
    RenderTexture {
        name: "TimelineRenderTexture".into(),
        width: 1024,
        height: 1024,
        anti_aliasing: 1,
        mip_count: -1,
        depth_stencil_format: 90,
        color_format: 8,
        mip_map: false,
        generate_mips: true,
        filter_mode: 1,
        aniso: 0,
        mip_bias: 0.0,
        wrap: [1, 1, 1],
        dimension: 2,
        volume_depth: 1,
        shadow_sampling: 2,
    }
}

#[test]
fn decodes_a_render_texture_laid_out_like_old_worlds() {
    assert_eq!(render_texture_bytes(false).len(), 100);
    let decoded =
        decode_one::<RenderTexture>(ClassId::RENDER_TEXTURE, render_texture_bytes(false), false)
            .unwrap();
    assert_eq!(decoded, expected());
}

#[test]
fn decodes_a_big_endian_render_texture() {
    let decoded =
        decode_one::<RenderTexture>(ClassId::RENDER_TEXTURE, render_texture_bytes(true), true)
            .unwrap();
    assert_eq!(decoded, expected());
}

#[test]
fn decodes_the_mip_and_generate_flags_independently() {
    let mut bytes = render_texture_bytes(false);
    // The flag bytes follow the name (4 + 21 + 3 padding), five ints and two format ints.
    let flags = 28 + 5 * 4 + 2 * 4;
    bytes[flags] = 1;
    bytes[flags + 1] = 0;
    let decoded = decode_one::<RenderTexture>(ClassId::RENDER_TEXTURE, bytes, false).unwrap();
    assert!(decoded.mip_map && !decoded.generate_mips);
}

#[test]
fn every_truncated_render_texture_fails() {
    let bytes = render_texture_bytes(false);
    for end in 0..bytes.len() {
        let error =
            decode_one::<RenderTexture>(ClassId::RENDER_TEXTURE, bytes[..end].to_vec(), false)
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
fn rejects_bytes_after_the_last_field() {
    let mut bytes = render_texture_bytes(false);
    bytes.push(0);
    let error = decode_one::<RenderTexture>(ClassId::RENDER_TEXTURE, bytes, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}
