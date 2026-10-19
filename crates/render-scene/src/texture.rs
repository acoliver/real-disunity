//! Validation and upload preparation for raw texels and standard DDS textures.
use thiserror::Error;
use wgpu::TextureFormat;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding};

/// Byte length of the DDS magic number.
const MAGIC_LEN: usize = 4;

/// The DDS magic number, `"DDS "`.
const MAGIC: &[u8; MAGIC_LEN] = b"DDS ";

/// Byte length of the base DDS header that follows the magic number.
const HEADER_LEN: usize = 124;

/// Byte offset of the base header's height field.
const OFFSET_HEIGHT: usize = 12;

/// Byte offset of the base header's width field.
const OFFSET_WIDTH: usize = 16;

/// Byte offset of the base header's mip count field.
const OFFSET_MIP_COUNT: usize = 28;

/// Byte offset of the pixel format's four-character code.
const OFFSET_FOURCC: usize = 84;

/// Byte offset of the extended header's `DXGI` format field.
const OFFSET_DXGI_FORMAT: usize = 128;

/// Byte length of a DDS file that carries an extended `DX10` header.
const DX10_HEADER_END: usize = 148;

/// The four-character code marking an extended `DX10` header.
const FOURCC_DX10: &[u8; 4] = b"DX10";

/// Edge length in texels of one block-compression block.
const BLOCK_EDGE: u32 = 4;

/// A governed texture could not be prepared for upload.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TextureError {
    /// The payload did not begin with the DDS magic number.
    #[error("texture payload is not a DDS container")]
    NotADdsContainer,
    /// The payload was too short to hold the header it claims.
    #[error("DDS header needs {needed} bytes but the payload holds {available}")]
    TruncatedHeader {
        /// Bytes the header requires.
        needed: usize,
        /// Bytes the payload provides.
        available: usize,
    },
    /// The container did not carry the extended header this decoder requires.
    #[error("DDS container lacks the DX10 extended header that declares its DXGI format")]
    MissingExtendedHeader,
    /// The stored `DXGI` format has no `wgpu` equivalent.
    #[error("DDS container declares unsupported DXGI format {code}")]
    UnsupportedDxgiFormat {
        /// The numeric `DXGI` format code found in the extended header.
        code: u32,
    },
    /// The container encoding is not one this renderer can upload.
    #[error("texture encoding {encoding:?} cannot be uploaded without decoding")]
    UnsupportedEncoding {
        /// The encoding the scene declared.
        encoding: TextureEncoding,
    },
    /// The domain colour space and the container format disagree.
    #[error(
        "texture declares {declared:?} but its DXGI format {code} stores {container:?}; \
         one of the two derivations has drifted"
    )]
    ColorSpaceDisagreement {
        /// The colour space the render-plan domain derived from the importer.
        declared: TextureColorSpace,
        /// The colour space implied by the stored format.
        container: TextureColorSpace,
        /// The numeric `DXGI` format code.
        code: u32,
    },
    /// A texel format cannot represent sRGB encoding on the GPU.
    #[error("texel format {format:?} has no sRGB variant, so {declared:?} cannot be honoured")]
    ColorSpaceUnrepresentable {
        /// The texel format the scene declared.
        format: TexelFormat,
        /// The colour space the scene declared.
        declared: TextureColorSpace,
    },
    /// A dimension was zero.
    #[error("texture dimensions {width}x{height} include a zero extent")]
    ZeroExtent {
        /// The declared width in texels.
        width: u32,
        /// The declared height in texels.
        height: u32,
    },
    /// The payload ran out before the declared mip chain did.
    #[error("mip level {level} needs {needed} bytes but only {available} remain")]
    TruncatedPayload {
        /// The mip level that could not be satisfied.
        level: u32,
        /// Bytes the level requires.
        needed: usize,
        /// Bytes remaining in the payload.
        available: usize,
    },
}

/// One mip level, borrowed from the archive rather than copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MipLevel<'a> {
    /// Width of this level in texels.
    pub width: u32,
    /// Height of this level in texels.
    pub height: u32,
    /// Bytes in one row of blocks, the stride a copy expects.
    pub bytes_per_row: u32,
    /// Number of block rows in this level.
    pub rows: u32,
    /// The level's payload, still block-compressed.
    pub bytes: &'a [u8],
}

/// A governed texture resolved to a `wgpu` format and a borrowed mip chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTexture<'a> {
    format: TextureFormat,
    width: u32,
    height: u32,
    mips: Vec<MipLevel<'a>>,
}

impl<'a> PreparedTexture<'a> {
    /// Returns the `wgpu` format, already carrying the resolved colour space.
    #[must_use]
    pub const fn format(&self) -> TextureFormat {
        self.format
    }

    /// Returns the width of mip zero in texels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of mip zero in texels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the mip chain, finest first.
    #[must_use]
    pub fn mips(&self) -> &[MipLevel<'a>] {
        &self.mips
    }
}

/// Resolves a governed texture into an uploadable form without decoding it.
///
/// # Errors
///
/// Returns [`TextureError`] when the container is malformed, stores a format
/// with no `wgpu` equivalent, or contradicts the colour space the scene
/// declared.
pub fn prepare_texture(texture: &SceneTexture) -> Result<PreparedTexture<'_>, TextureError> {
    match texture {
        SceneTexture::Texels {
            format,
            width,
            height,
            mips,
            bytes,
            ..
        } => prepare_texels(
            *format,
            texture.color_space(),
            *width,
            *height,
            *mips,
            bytes,
        ),
        SceneTexture::Encoded {
            encoding, bytes, ..
        } => {
            if *encoding != TextureEncoding::DdsContainer {
                return Err(TextureError::UnsupportedEncoding {
                    encoding: *encoding,
                });
            }
            prepare_dds(bytes, texture.color_space())
        }
    }
}

/// Resolves an uncompressed texel payload into a single-level texture.
fn prepare_texels(
    format: TexelFormat,
    declared: TextureColorSpace,
    width: u32,
    height: u32,
    mip_count: u32,
    bytes: &[u8],
) -> Result<PreparedTexture<'_>, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroExtent { width, height });
    }
    let gpu_format = texel_format(format, declared)?;
    let bytes_per_texel = u32::try_from(format.bytes_per_texel()).unwrap_or(u32::MAX);
    let mut offset = 0usize;
    let mut mips = Vec::with_capacity(usize::try_from(mip_count).unwrap_or(0));
    for level in 0..mip_count {
        let mip_width = width.checked_shr(level).unwrap_or(0).max(1);
        let mip_height = height.checked_shr(level).unwrap_or(0).max(1);
        let bytes_per_row = bytes_per_texel.saturating_mul(mip_width);
        let level_bytes =
            usize::try_from(u64::from(bytes_per_row) * u64::from(mip_height)).unwrap_or(usize::MAX);
        let available = bytes.len().saturating_sub(offset);
        if level_bytes > available {
            return Err(TextureError::TruncatedPayload {
                level,
                needed: level_bytes,
                available,
            });
        }
        mips.push(MipLevel {
            width: mip_width,
            height: mip_height,
            bytes_per_row,
            rows: mip_height,
            bytes: &bytes[offset..offset + level_bytes],
        });
        offset += level_bytes;
    }
    Ok(PreparedTexture {
        format: gpu_format,
        width,
        height,
        mips,
    })
}

/// Maps an uncompressed texel format onto its `wgpu` equivalent.
///
/// Single- and dual-channel formats have no sRGB variant in any graphics API,
/// because sRGB is a colour encoding and those formats carry data. Silently
/// treating such a texture as linear would be a plausible-looking guess that
/// contradicts what the scene declared, so the mismatch is reported instead.
fn texel_format(
    format: TexelFormat,
    declared: TextureColorSpace,
) -> Result<TextureFormat, TextureError> {
    match (format, declared) {
        (TexelFormat::R8Unorm, TextureColorSpace::Linear) => Ok(TextureFormat::R8Unorm),
        (TexelFormat::Rg8Unorm, TextureColorSpace::Linear) => Ok(TextureFormat::Rg8Unorm),
        (TexelFormat::Rgba8Unorm, TextureColorSpace::Linear) => Ok(TextureFormat::Rgba8Unorm),
        (TexelFormat::Rgba8Unorm, TextureColorSpace::Srgb) => Ok(TextureFormat::Rgba8UnormSrgb),
        (TexelFormat::Bgra8Unorm, TextureColorSpace::Linear) => Ok(TextureFormat::Bgra8Unorm),
        (TexelFormat::Bgra8Unorm, TextureColorSpace::Srgb) => Ok(TextureFormat::Bgra8UnormSrgb),
        (format @ (TexelFormat::R8Unorm | TexelFormat::Rg8Unorm), declared) => {
            Err(TextureError::ColorSpaceUnrepresentable { format, declared })
        }
    }
}

/// Parses a DDS container and borrows its mip chain.
fn prepare_dds(
    bytes: &[u8],
    declared: TextureColorSpace,
) -> Result<PreparedTexture<'_>, TextureError> {
    if bytes.len() < MAGIC_LEN || &bytes[..MAGIC_LEN] != MAGIC {
        return Err(TextureError::NotADdsContainer);
    }
    if bytes.len() < MAGIC_LEN + HEADER_LEN {
        return Err(TextureError::TruncatedHeader {
            needed: MAGIC_LEN + HEADER_LEN,
            available: bytes.len(),
        });
    }
    if read_u32(bytes, OFFSET_FOURCC).to_le_bytes() != *FOURCC_DX10 {
        return Err(TextureError::MissingExtendedHeader);
    }
    if bytes.len() < DX10_HEADER_END {
        return Err(TextureError::TruncatedHeader {
            needed: DX10_HEADER_END,
            available: bytes.len(),
        });
    }

    let width = read_u32(bytes, OFFSET_WIDTH);
    let height = read_u32(bytes, OFFSET_HEIGHT);
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroExtent { width, height });
    }
    let code = read_u32(bytes, OFFSET_DXGI_FORMAT);
    let (format, block_bytes, container_space) =
        dxgi_format(code).ok_or(TextureError::UnsupportedDxgiFormat { code })?;
    if container_space != declared {
        return Err(TextureError::ColorSpaceDisagreement {
            declared,
            container: container_space,
            code,
        });
    }

    let mips = slice_mips(
        &bytes[DX10_HEADER_END..],
        width,
        height,
        read_u32(bytes, OFFSET_MIP_COUNT).max(1),
        block_bytes,
    )?;
    Ok(PreparedTexture {
        format,
        width,
        height,
        mips,
    })
}

/// Divides a payload into the mip levels its header declares.
fn slice_mips(
    payload: &[u8],
    width: u32,
    height: u32,
    mip_count: u32,
    block_bytes: u32,
) -> Result<Vec<MipLevel<'_>>, TextureError> {
    let mut mips = Vec::with_capacity(usize::try_from(mip_count).unwrap_or(1));
    let mut offset = 0usize;
    for level in 0..mip_count {
        let level_width = (width >> level).max(1);
        let level_height = (height >> level).max(1);
        let bytes_per_row = level_width.div_ceil(BLOCK_EDGE).max(1) * block_bytes;
        let rows = level_height.div_ceil(BLOCK_EDGE).max(1);
        let needed =
            usize::try_from(u64::from(bytes_per_row) * u64::from(rows)).unwrap_or(usize::MAX);
        let available = payload.len().saturating_sub(offset);
        if needed > available {
            return Err(TextureError::TruncatedPayload {
                level,
                needed,
                available,
            });
        }
        mips.push(MipLevel {
            width: level_width,
            height: level_height,
            bytes_per_row,
            rows,
            bytes: &payload[offset..offset + needed],
        });
        offset += needed;
    }
    Ok(mips)
}

const fn dxgi_format(code: u32) -> Option<(TextureFormat, u32, TextureColorSpace)> {
    match code {
        71 => Some((TextureFormat::Bc1RgbaUnorm, 8, TextureColorSpace::Linear)),
        72 => Some((TextureFormat::Bc1RgbaUnormSrgb, 8, TextureColorSpace::Srgb)),
        77 => Some((TextureFormat::Bc3RgbaUnorm, 16, TextureColorSpace::Linear)),
        78 => Some((TextureFormat::Bc3RgbaUnormSrgb, 16, TextureColorSpace::Srgb)),
        80 => Some((TextureFormat::Bc4RUnorm, 8, TextureColorSpace::Linear)),
        83 => Some((TextureFormat::Bc5RgUnorm, 16, TextureColorSpace::Linear)),
        _ => None,
    }
}

/// Reads a little-endian `u32` from a checked offset.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(word)
}

#[cfg(test)]
mod tests;
