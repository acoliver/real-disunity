//! Behavioural claims about preparing governed textures for upload.

use super::{TextureError, prepare_texture};
use wgpu::TextureFormat;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding};

/// `DXGI_FORMAT_BC1_UNORM`, the linear four-channel block format.
const DXGI_BC1_UNORM: u32 = 71;

/// `DXGI_FORMAT_BC1_UNORM_SRGB`, its sRGB-encoded counterpart.
const DXGI_BC1_UNORM_SRGB: u32 = 72;

/// `DXGI_FORMAT_BC3_UNORM_SRGB`, the sRGB block format carrying alpha.
const DXGI_BC3_UNORM_SRGB: u32 = 78;

/// `DXGI_FORMAT_BC5_UNORM`, the two-channel format normal maps use.
const DXGI_BC5_UNORM: u32 = 83;

const DXGI_BC7_UNORM: u32 = 98;

/// Builds a DX10 DDS container around a payload of the requested length.
fn dds(dxgi: u32, width: u32, height: u32, mip_count: u32, payload: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(148 + payload);
    bytes.extend_from_slice(b"DDS ");
    bytes.extend_from_slice(&124u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&mip_count.to_le_bytes());
    bytes.resize(76, 0);
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.extend_from_slice(&0x0004u32.to_le_bytes());
    bytes.extend_from_slice(b"DX10");
    bytes.resize(4 + 124, 0);
    bytes.extend_from_slice(&dxgi.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(bytes.len(), 148, "the DX10 header is 148 bytes");
    bytes.resize(148 + payload, 0xCD);
    bytes
}

fn encoded(dxgi: u32, space: TextureColorSpace, payload: usize) -> SceneTexture {
    encoded_sized(dxgi, space, 8, 8, 1, payload)
}

fn encoded_sized(
    dxgi: u32,
    space: TextureColorSpace,
    width: u32,
    height: u32,
    mip_count: u32,
    payload: usize,
) -> SceneTexture {
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: space,
        bytes: dds(dxgi, width, height, mip_count, payload).into(),
    }
}

#[test]
fn an_srgb_container_resolves_to_an_srgb_gpu_format() {
    // The entire point of carrying TextureColorSpace through the domain: the
    // sampler must gamma-decode colour and must not touch data.
    let texture = encoded(DXGI_BC1_UNORM_SRGB, TextureColorSpace::Srgb, 32);
    let prepared = prepare_texture(&texture).expect("a well-formed BC1 sRGB container prepares");
    assert_eq!(prepared.format(), TextureFormat::Bc1RgbaUnormSrgb);
}

#[test]
fn a_linear_container_resolves_to_a_linear_gpu_format() {
    let texture = encoded(DXGI_BC1_UNORM, TextureColorSpace::Linear, 32);
    let prepared = prepare_texture(&texture).expect("a well-formed BC1 linear container prepares");
    assert_eq!(prepared.format(), TextureFormat::Bc1RgbaUnorm);
}

#[test]
fn a_normal_map_resolves_to_the_two_channel_block_format() {
    let texture = encoded(DXGI_BC5_UNORM, TextureColorSpace::Linear, 64);
    let prepared = prepare_texture(&texture).expect("a well-formed BC5 container prepares");
    assert_eq!(prepared.format(), TextureFormat::Bc5RgUnorm);
}

#[test]
fn the_payload_is_borrowed_rather_than_decoded() {
    // Preserve block compression and avoid allocating an expanded RGBA copy:
    // the prepared mip must alias the container's own bytes.
    let texture = encoded(DXGI_BC3_UNORM_SRGB, TextureColorSpace::Srgb, 64);
    let prepared = prepare_texture(&texture).expect("a well-formed BC3 container prepares");
    let SceneTexture::Encoded { bytes, .. } = &texture else {
        unreachable!("the fixture is an encoded texture");
    };
    let mip = prepared.mips()[0];
    assert_eq!(mip.bytes.as_ptr(), bytes[148..].as_ptr());
    assert!(
        bytes[148..].len() >= mip.bytes.len(),
        "the mip must lie inside the container payload"
    );
}

#[test]
fn a_disagreement_between_importer_and_container_is_reported() {
    let texture = encoded(DXGI_BC1_UNORM, TextureColorSpace::Srgb, 32);
    let error = prepare_texture(&texture).expect_err("a colour-space disagreement is reported");
    assert_eq!(
        error,
        TextureError::ColorSpaceDisagreement {
            declared: TextureColorSpace::Srgb,
            container: TextureColorSpace::Linear,
            code: DXGI_BC1_UNORM,
        }
    );
}

#[test]
fn an_unmapped_block_format_is_rejected_rather_than_approximated() {
    // BC7 is a plausible near-neighbour of BC1 and BC3 and decodes to the same
    // channels, but its block layout differs, so substituting one for another
    // yields noise rather than an approximation.
    let texture = encoded(DXGI_BC7_UNORM, TextureColorSpace::Linear, 64);
    let error = prepare_texture(&texture).expect_err("an unmapped DXGI format is reported");
    assert_eq!(
        error,
        TextureError::UnsupportedDxgiFormat {
            code: DXGI_BC7_UNORM
        }
    );
}

#[test]
fn the_mip_chain_is_sliced_at_block_aligned_strides() {
    // A 16x16 BC1 image is 4x4 blocks of 8 bytes at mip 0, then 2x2, then 1x1,
    // then 1x1 again as the chain clamps rather than reaching zero.
    let payload = 128 + 32 + 8 + 8;
    let texture = encoded_sized(
        DXGI_BC1_UNORM,
        TextureColorSpace::Linear,
        16,
        16,
        4,
        payload,
    );
    let prepared = prepare_texture(&texture).expect("a four-level BC1 chain prepares");
    let mips = prepared.mips();
    assert_eq!(mips.len(), 4);
    assert_eq!((mips[0].width, mips[0].height), (16, 16));
    assert_eq!((mips[0].bytes_per_row, mips[0].rows), (32, 4));
    assert_eq!(mips[0].bytes.len(), 128);
    assert_eq!((mips[3].width, mips[3].height), (2, 2));
    assert_eq!(
        (mips[3].bytes_per_row, mips[3].rows),
        (8, 1),
        "a level smaller than one block still occupies a whole block"
    );
}

#[test]
fn a_truncated_mip_chain_is_reported_against_the_level_that_ran_out() {
    let texture = encoded_sized(
        DXGI_BC1_UNORM,
        TextureColorSpace::Linear,
        16,
        16,
        4,
        128 + 16,
    );
    let error = prepare_texture(&texture).expect_err("a short payload is reported");
    assert_eq!(
        error,
        TextureError::TruncatedPayload {
            level: 1,
            needed: 32,
            available: 16,
        }
    );
}

#[test]
fn a_header_without_the_extended_block_is_rejected() {
    let mut bytes = dds(DXGI_BC1_UNORM, 8, 8, 1, 32);
    bytes[88..92].copy_from_slice(b"DXT1");
    bytes[84..88].copy_from_slice(b"DXT1");
    let texture = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into(),
    };
    let error = prepare_texture(&texture).expect_err("a legacy header is reported");
    assert_eq!(error, TextureError::MissingExtendedHeader);
}

#[test]
fn a_payload_that_is_not_a_dds_container_is_rejected() {
    let texture = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: b"not a dds file at all".to_vec().into(),
    };
    let error = prepare_texture(&texture).expect_err("a foreign payload is reported");
    assert_eq!(error, TextureError::NotADdsContainer);
}

#[test]
fn a_container_encoding_this_renderer_cannot_upload_is_reported() {
    let texture = SceneTexture::Encoded {
        encoding: TextureEncoding::KtxContainer,
        color_space: TextureColorSpace::Linear,
        bytes: Vec::new().into(),
    };
    let error = prepare_texture(&texture).expect_err("an unsupported encoding is reported");
    assert_eq!(
        error,
        TextureError::UnsupportedEncoding {
            encoding: TextureEncoding::KtxContainer
        }
    );
}

#[test]
fn uncompressed_srgb_texels_resolve_to_the_srgb_variant() {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![0u8; 16].into(),
    };
    let prepared = prepare_texture(&texture).expect("uncompressed texels prepare");
    assert_eq!(prepared.format(), TextureFormat::Rgba8UnormSrgb);
    assert_eq!(prepared.mips()[0].bytes_per_row, 8);
}

#[test]
fn a_single_channel_format_cannot_claim_to_be_srgb() {
    // No graphics API offers an sRGB single-channel format, because sRGB is a
    // colour encoding and one channel is not a colour. Quietly sampling it as
    // linear would contradict what the scene declared.
    let texture = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![0u8; 4].into(),
    };
    let error = prepare_texture(&texture).expect_err("an unrepresentable colour space is reported");
    assert_eq!(
        error,
        TextureError::ColorSpaceUnrepresentable {
            format: TexelFormat::R8Unorm,
            declared: TextureColorSpace::Srgb,
        }
    );
}

#[test]
fn raw_npot_texels_are_split_into_floor_halved_mips() {
    // GPU mip dimensions halve with floor division: 5x3 -> 2x1 -> 1x1.
    // Distinct bytes make both the boundaries and each borrowed slice visible.
    let bytes: Vec<u8> = (0..18 * 4)
        .map(|byte| u8::try_from(byte).expect("fixture byte fits"))
        .collect();
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 5,
        height: 3,
        mips: 3,
        bytes: bytes.clone().into(),
    };

    let prepared = prepare_texture(&texture).expect("the complete raw mip chain prepares");
    let mips = prepared.mips();
    assert_eq!(
        mips.iter()
            .map(|mip| (mip.width, mip.height))
            .collect::<Vec<_>>(),
        [(5, 3), (2, 1), (1, 1)]
    );
    assert_eq!(
        mips.iter().map(|mip| mip.bytes_per_row).collect::<Vec<_>>(),
        [20, 8, 4]
    );
    assert_eq!(
        mips.iter().map(|mip| mip.bytes.len()).collect::<Vec<_>>(),
        [60, 8, 4]
    );
    assert_eq!(mips[0].bytes, &bytes[..60]);
    assert_eq!(mips[1].bytes, &bytes[60..68]);
    assert_eq!(mips[2].bytes, &bytes[68..]);
}
