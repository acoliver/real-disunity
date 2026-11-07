use worldrunner_texture::{TextureError, TextureFormat};

fn header(code: u32) -> Vec<u8> {
    let mut bytes = vec![0; 148];
    for (offset, word) in [
        (4, 124_u32),
        (12, 4),
        (16, 4),
        (28, 1),
        (128, code),
        (132, 3),
        (140, 1),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[84..88].copy_from_slice(b"DX10");
    bytes
}

#[test]
fn dds_format_admission_reads_metadata_without_a_payload_copy() {
    for (code, format, channels) in [
        (71, TextureFormat::Bc1Unorm, 4),
        (72, TextureFormat::Bc1UnormSrgb, 4),
        (77, TextureFormat::Bc3Unorm, 4),
        (78, TextureFormat::Bc3UnormSrgb, 4),
        (80, TextureFormat::Bc4Unorm, 1),
        (81, TextureFormat::Bc4Snorm, 1),
        (83, TextureFormat::Bc5Unorm, 2),
        (84, TextureFormat::Bc5Snorm, 2),
        (98, TextureFormat::Bc7Unorm, 4),
        (99, TextureFormat::Bc7UnormSrgb, 4),
        (28, TextureFormat::R8g8b8a8Unorm, 4),
    ] {
        assert_eq!(TextureFormat::from_dx10_dds(&header(code)), Ok(format));
        assert_eq!(format.channel_count(), channels);
    }
    assert_eq!(TextureFormat::R8g8b8a8UnormSrgb.channel_count(), 4);
}

#[test]
fn metadata_admission_preserves_header_errors() {
    assert_eq!(
        TextureFormat::from_dx10_dds(&[]),
        Err(TextureError::TruncatedDds)
    );
    assert_eq!(
        TextureFormat::from_dx10_dds(&header(97)),
        Err(TextureError::UnsupportedDxgiFormat(97))
    );
    let mut bytes = header(98);
    bytes[140..144].copy_from_slice(&2_u32.to_le_bytes());
    assert_eq!(
        TextureFormat::from_dx10_dds(&bytes),
        Err(TextureError::UnsupportedDdsLayout)
    );
}
