use serde::{Deserialize, Serialize};
use thiserror::Error;

const DDS_MAGIC: [u8; 4] = *b"DDS ";
const DDS_HEADER_BYTES: usize = 128;
const DDS_DX10_HEADER_BYTES: usize = 20;
const DDS_DX10_PAYLOAD_OFFSET: usize = DDS_HEADER_BYTES + DDS_DX10_HEADER_BYTES;
const FOURCC_DX10: [u8; 4] = *b"DX10";
const TEXTURE_MAGIC: [u8; 8] = *b"RDTEX001";
const TEXTURE_VERSION: u32 = 1;
const TEXTURE_HEADER_BYTES: usize = 44;
const MIP_DESCRIPTOR_BYTES: usize = 24;
const D3D10_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
const D3D10_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const DXGI_FORMAT_BC1_UNORM_SRGB: u32 = 72;
const DXGI_FORMAT_BC3_UNORM: u32 = 77;
const DXGI_FORMAT_BC3_UNORM_SRGB: u32 = 78;
const DXGI_FORMAT_BC4_UNORM: u32 = 80;
const DXGI_FORMAT_BC4_SNORM: u32 = 81;
const DXGI_FORMAT_BC5_UNORM: u32 = 83;
const DXGI_FORMAT_BC5_SNORM: u32 = 84;
const DXGI_FORMAT_BC7_UNORM: u32 = 98;
const DXGI_FORMAT_BC7_UNORM_SRGB: u32 = 99;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u32)]
pub enum TextureFormat {
    Bc3Unorm = 1,
    Bc3UnormSrgb = 2,
    Bc1Unorm = 3,
    Bc1UnormSrgb = 4,
    Bc5Unorm = 5,
    Bc5Snorm = 6,
    Bc7Unorm = 7,
    Bc7UnormSrgb = 8,
    Bc4Unorm = 9,
    Bc4Snorm = 10,
    R8g8b8a8Unorm = 11,
    R8g8b8a8UnormSrgb = 12,
}

impl TextureFormat {
    const fn from_dxgi(value: u32) -> Result<Self, TextureError> {
        match value {
            DXGI_FORMAT_BC1_UNORM => Ok(Self::Bc1Unorm),
            DXGI_FORMAT_BC1_UNORM_SRGB => Ok(Self::Bc1UnormSrgb),
            DXGI_FORMAT_BC3_UNORM => Ok(Self::Bc3Unorm),
            DXGI_FORMAT_BC3_UNORM_SRGB => Ok(Self::Bc3UnormSrgb),
            DXGI_FORMAT_BC4_UNORM => Ok(Self::Bc4Unorm),
            DXGI_FORMAT_BC4_SNORM => Ok(Self::Bc4Snorm),
            DXGI_FORMAT_BC5_UNORM => Ok(Self::Bc5Unorm),
            DXGI_FORMAT_BC5_SNORM => Ok(Self::Bc5Snorm),
            DXGI_FORMAT_BC7_UNORM => Ok(Self::Bc7Unorm),
            DXGI_FORMAT_BC7_UNORM_SRGB => Ok(Self::Bc7UnormSrgb),
            DXGI_FORMAT_R8G8B8A8_UNORM => Ok(Self::R8g8b8a8Unorm),
            _ => Err(TextureError::UnsupportedDxgiFormat(value)),
        }
    }

    const fn from_artifact(value: u32) -> Result<Self, TextureError> {
        match value {
            1 => Ok(Self::Bc3Unorm),
            2 => Ok(Self::Bc3UnormSrgb),
            3 => Ok(Self::Bc1Unorm),
            4 => Ok(Self::Bc1UnormSrgb),
            5 => Ok(Self::Bc5Unorm),
            6 => Ok(Self::Bc5Snorm),
            7 => Ok(Self::Bc7Unorm),
            8 => Ok(Self::Bc7UnormSrgb),
            9 => Ok(Self::Bc4Unorm),
            10 => Ok(Self::Bc4Snorm),
            11 => Ok(Self::R8g8b8a8Unorm),
            12 => Ok(Self::R8g8b8a8UnormSrgb),
            _ => Err(TextureError::UnsupportedArtifactFormat(value)),
        }
    }

    const fn block_bytes(self) -> u64 {
        match self {
            Self::Bc1Unorm | Self::Bc1UnormSrgb | Self::Bc4Unorm | Self::Bc4Snorm => 8,
            Self::Bc3Unorm
            | Self::Bc3UnormSrgb
            | Self::Bc5Unorm
            | Self::Bc5Snorm
            | Self::Bc7Unorm
            | Self::Bc7UnormSrgb => 16,
            // R8G8B8A8 is not block-compressed; block_bytes is unused for it (see
            // bytes_per_mip). A sentinel of 0 documents that no block padding applies.
            Self::R8g8b8a8Unorm | Self::R8g8b8a8UnormSrgb => 0,
        }
    }

    /// Returns `true` when the format is block-compressed (4x4 texel blocks). Uncompressed
    /// formats like R8G8B8A8 use a direct per-texel stride instead.
    pub const fn is_block_compressed(self) -> bool {
        !matches!(self, Self::R8g8b8a8Unorm | Self::R8g8b8a8UnormSrgb)
    }

    /// Computes the payload byte length for a single mip level of the given dimensions.
    /// Block-compressed formats use 4x4 texel blocks; R8G8B8A8 uses 4 bytes per texel.
    #[allow(clippy::manual_div_ceil)]
    const fn bytes_per_mip(self, width: u32, height: u32) -> u64 {
        if self.is_block_compressed() {
            let blocks_wide = ((width as u64) + 3) / 4;
            let blocks_high = ((height as u64) + 3) / 4;
            blocks_wide * blocks_high * self.block_bytes()
        } else {
            (width as u64) * (height as u64) * 4
        }
    }

    const fn is_srgb(self) -> bool {
        matches!(
            self,
            Self::Bc1UnormSrgb | Self::Bc3UnormSrgb | Self::Bc7UnormSrgb
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u32)]
pub enum TextureSemantic {
    Color = 1,
    Normal = 2,
    Data = 3,
}

impl TextureSemantic {
    const fn from_artifact(value: u32) -> Result<Self, TextureError> {
        match value {
            1 => Ok(Self::Color),
            2 => Ok(Self::Normal),
            3 => Ok(Self::Data),
            _ => Err(TextureError::UnsupportedSemantic(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureDescriptor {
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub array_layers: u32,
    pub format: TextureFormat,
    pub semantic: TextureSemantic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MipDescriptor {
    pub width: u32,
    pub height: u32,
    pub payload_offset: u64,
    pub payload_length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureArtifact {
    pub descriptor: TextureDescriptor,
    pub mips: Vec<MipDescriptor>,
    pub payload: Vec<u8>,
}

/// The subset of the DDS/DX10 header pair this crate accepts.
struct Dx10Header {
    width: u32,
    height: u32,
    mip_count: u32,
    array_layers: u32,
    format: TextureFormat,
}

impl Dx10Header {
    fn parse(bytes: &[u8]) -> Result<Self, TextureError> {
        if bytes.len() < DDS_DX10_PAYLOAD_OFFSET {
            return Err(TextureError::TruncatedDds);
        }
        if bytes[..4] != DDS_MAGIC || read_u32(bytes, 4)? != 124 {
            return Err(TextureError::InvalidDdsHeader);
        }
        if bytes[84..88] != FOURCC_DX10 {
            return Err(TextureError::UnsupportedDdsLayout);
        }
        let width = read_u32(bytes, 16)?;
        let height = read_u32(bytes, 12)?;
        let mip_count = read_u32(bytes, 28)?.max(1);
        let format = TextureFormat::from_dxgi(read_u32(bytes, DDS_HEADER_BYTES)?)?;
        let dimension = read_u32(bytes, DDS_HEADER_BYTES + 4)?;
        let misc_flags = read_u32(bytes, DDS_HEADER_BYTES + 8)?;
        let array_layers = read_u32(bytes, DDS_HEADER_BYTES + 12)?;
        if width == 0 || height == 0 || array_layers == 0 {
            return Err(TextureError::InvalidDimensions);
        }
        if dimension != D3D10_RESOURCE_DIMENSION_TEXTURE2D
            || array_layers != 1
            || misc_flags & D3D10_RESOURCE_MISC_TEXTURECUBE != 0
        {
            return Err(TextureError::UnsupportedDdsLayout);
        }
        if mip_count > 32 {
            return Err(TextureError::TooManyMips(mip_count));
        }
        Ok(Self {
            width,
            height,
            mip_count,
            array_layers,
            format,
        })
    }
}

/// Lays out the mip chain, returning the descriptors and the total payload size.
fn describe_mip_chain(
    format: TextureFormat,
    width: u32,
    height: u32,
    mip_count: u32,
) -> Result<(Vec<MipDescriptor>, u64), TextureError> {
    let mut mips =
        Vec::with_capacity(usize::try_from(mip_count).map_err(|_| TextureError::SizeOverflow)?);
    let mut payload_offset = 0_u64;
    let mut mip_width = width;
    let mut mip_height = height;
    for _ in 0..mip_count {
        let payload_length = format.bytes_per_mip(mip_width, mip_height);
        mips.push(MipDescriptor {
            width: mip_width,
            height: mip_height,
            payload_offset,
            payload_length,
        });
        payload_offset = payload_offset
            .checked_add(payload_length)
            .ok_or(TextureError::SizeOverflow)?;
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }
    Ok((mips, payload_offset))
}

impl TextureArtifact {
    pub fn from_dx10_dds(bytes: &[u8], semantic: TextureSemantic) -> Result<Self, TextureError> {
        let Dx10Header {
            width,
            height,
            mip_count,
            array_layers,
            format,
        } = Dx10Header::parse(bytes)?;
        let (mips, payload_offset) = describe_mip_chain(format, width, height, mip_count)?;
        let expected_payload =
            usize::try_from(payload_offset).map_err(|_| TextureError::SizeOverflow)?;
        let payload = bytes
            .get(DDS_DX10_PAYLOAD_OFFSET..)
            .ok_or(TextureError::TruncatedDds)?;
        if payload.len() != expected_payload {
            return Err(TextureError::PayloadLength {
                expected: expected_payload,
                actual: payload.len(),
            });
        }
        if semantic == TextureSemantic::Color && !format.is_srgb() {
            return Err(TextureError::SemanticFormatMismatch);
        }
        if semantic != TextureSemantic::Color && format.is_srgb() {
            return Err(TextureError::SemanticFormatMismatch);
        }
        Ok(Self {
            descriptor: TextureDescriptor {
                width,
                height,
                mip_count,
                array_layers,
                format,
                semantic,
            },
            mips,
            payload: payload.to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, TextureError> {
        self.validate()?;
        let descriptors_length = self
            .mips
            .len()
            .checked_mul(MIP_DESCRIPTOR_BYTES)
            .ok_or(TextureError::SizeOverflow)?;
        let mut output = Vec::with_capacity(
            TEXTURE_HEADER_BYTES
                .checked_add(descriptors_length)
                .and_then(|size| size.checked_add(self.payload.len()))
                .ok_or(TextureError::SizeOverflow)?,
        );
        output.extend_from_slice(&TEXTURE_MAGIC);
        output.extend_from_slice(&TEXTURE_VERSION.to_le_bytes());
        output.extend_from_slice(&self.descriptor.width.to_le_bytes());
        output.extend_from_slice(&self.descriptor.height.to_le_bytes());
        output.extend_from_slice(&self.descriptor.mip_count.to_le_bytes());
        output.extend_from_slice(&self.descriptor.array_layers.to_le_bytes());
        output.extend_from_slice(&(self.descriptor.format as u32).to_le_bytes());
        output.extend_from_slice(&(self.descriptor.semantic as u32).to_le_bytes());
        output.extend_from_slice(
            &u64::try_from(self.payload.len())
                .map_err(|_| TextureError::SizeOverflow)?
                .to_le_bytes(),
        );
        for mip in &self.mips {
            output.extend_from_slice(&mip.width.to_le_bytes());
            output.extend_from_slice(&mip.height.to_le_bytes());
            output.extend_from_slice(&mip.payload_offset.to_le_bytes());
            output.extend_from_slice(&mip.payload_length.to_le_bytes());
        }
        output.extend_from_slice(&self.payload);
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, TextureError> {
        if bytes.len() < TEXTURE_HEADER_BYTES {
            return Err(TextureError::TruncatedArtifact);
        }
        if bytes[..8] != TEXTURE_MAGIC {
            return Err(TextureError::InvalidArtifactMagic);
        }
        let version = read_u32(bytes, 8)?;
        if version != TEXTURE_VERSION {
            return Err(TextureError::UnsupportedArtifactVersion(version));
        }
        let width = read_u32(bytes, 12)?;
        let height = read_u32(bytes, 16)?;
        let mip_count = read_u32(bytes, 20)?;
        let array_layers = read_u32(bytes, 24)?;
        let format = TextureFormat::from_artifact(read_u32(bytes, 28)?)?;
        let semantic = TextureSemantic::from_artifact(read_u32(bytes, 32)?)?;
        let payload_length =
            usize::try_from(read_u64(bytes, 36)?).map_err(|_| TextureError::SizeOverflow)?;
        let mip_count_usize = usize::try_from(mip_count).map_err(|_| TextureError::SizeOverflow)?;
        let payload_start = TEXTURE_HEADER_BYTES
            .checked_add(
                mip_count_usize
                    .checked_mul(MIP_DESCRIPTOR_BYTES)
                    .ok_or(TextureError::SizeOverflow)?,
            )
            .ok_or(TextureError::SizeOverflow)?;
        let payload_end = payload_start
            .checked_add(payload_length)
            .ok_or(TextureError::SizeOverflow)?;
        if payload_end != bytes.len() {
            return Err(TextureError::InvalidArtifactExtent);
        }
        let mut mips = Vec::with_capacity(mip_count_usize);
        for index in 0..mip_count_usize {
            let offset = TEXTURE_HEADER_BYTES + index * MIP_DESCRIPTOR_BYTES;
            mips.push(MipDescriptor {
                width: read_u32(bytes, offset)?,
                height: read_u32(bytes, offset + 4)?,
                payload_offset: read_u64(bytes, offset + 8)?,
                payload_length: read_u64(bytes, offset + 16)?,
            });
        }
        let artifact = Self {
            descriptor: TextureDescriptor {
                width,
                height,
                mip_count,
                array_layers,
                format,
                semantic,
            },
            mips,
            payload: bytes[payload_start..payload_end].to_vec(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    fn validate(&self) -> Result<(), TextureError> {
        if self.descriptor.width == 0
            || self.descriptor.height == 0
            || self.descriptor.array_layers != 1
            || self.descriptor.mip_count == 0
        {
            return Err(TextureError::InvalidDimensions);
        }
        if self.mips.len()
            != usize::try_from(self.descriptor.mip_count).map_err(|_| TextureError::SizeOverflow)?
        {
            return Err(TextureError::InvalidMipTable);
        }
        let mut expected_offset = 0_u64;
        let mut expected_width = self.descriptor.width;
        let mut expected_height = self.descriptor.height;
        for mip in &self.mips {
            if mip.width != expected_width
                || mip.height != expected_height
                || mip.payload_offset != expected_offset
            {
                return Err(TextureError::InvalidMipTable);
            }
            expected_offset = expected_offset
                .checked_add(mip.payload_length)
                .ok_or(TextureError::SizeOverflow)?;
            expected_width = (expected_width / 2).max(1);
            expected_height = (expected_height / 2).max(1);
        }
        if usize::try_from(expected_offset).map_err(|_| TextureError::SizeOverflow)?
            != self.payload.len()
        {
            return Err(TextureError::InvalidMipTable);
        }
        Ok(())
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, TextureError> {
    bytes
        .get(offset..offset + 4)
        .ok_or(TextureError::TruncatedData)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| TextureError::TruncatedData)
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, TextureError> {
    bytes
        .get(offset..offset + 8)
        .ok_or(TextureError::TruncatedData)?
        .try_into()
        .map(u64::from_le_bytes)
        .map_err(|_| TextureError::TruncatedData)
}

#[derive(Debug, Error, PartialEq)]
pub enum TextureError {
    #[error("DDS data is truncated")]
    TruncatedDds,
    #[error("DDS header is invalid")]
    InvalidDdsHeader,
    #[error("DDS layout is unsupported")]
    UnsupportedDdsLayout,
    #[error("DDS DXGI format {0} is unsupported")]
    UnsupportedDxgiFormat(u32),
    #[error("DDS dimensions are invalid")]
    InvalidDimensions,
    #[error("DDS mip count {0} exceeds the supported limit")]
    TooManyMips(u32),
    #[error("DDS payload expected {expected} bytes but contains {actual}")]
    PayloadLength { expected: usize, actual: usize },
    #[error("texture semantic and pixel format do not agree")]
    SemanticFormatMismatch,
    #[error("texture size arithmetic overflowed")]
    SizeOverflow,
    #[error("texture artifact is truncated")]
    TruncatedArtifact,
    #[error("texture artifact magic is invalid")]
    InvalidArtifactMagic,
    #[error("texture artifact version {0} is unsupported")]
    UnsupportedArtifactVersion(u32),
    #[error("texture artifact format {0} is unsupported")]
    UnsupportedArtifactFormat(u32),
    #[error("texture semantic {0} is unsupported")]
    UnsupportedSemantic(u32),
    #[error("texture artifact extent is invalid")]
    InvalidArtifactExtent,
    #[error("texture mip table is invalid")]
    InvalidMipTable,
    #[error("texture data is truncated")]
    TruncatedData,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds(format: u32) -> Vec<u8> {
        let payload_bytes =
            usize::try_from(TextureFormat::from_dxgi(format).unwrap().block_bytes()).unwrap();
        let mut bytes = vec![0_u8; DDS_DX10_PAYLOAD_OFFSET + payload_bytes];
        bytes[..4].copy_from_slice(&DDS_MAGIC);
        bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&4_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&4_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&1_u32.to_le_bytes());
        bytes[84..88].copy_from_slice(&FOURCC_DX10);
        bytes[DDS_HEADER_BYTES..DDS_HEADER_BYTES + 4].copy_from_slice(&format.to_le_bytes());
        bytes[DDS_HEADER_BYTES + 4..DDS_HEADER_BYTES + 8]
            .copy_from_slice(&D3D10_RESOURCE_DIMENSION_TEXTURE2D.to_le_bytes());
        bytes[DDS_HEADER_BYTES + 12..DDS_HEADER_BYTES + 16].copy_from_slice(&1_u32.to_le_bytes());
        for (index, byte) in bytes[DDS_DX10_PAYLOAD_OFFSET..].iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap();
        }
        bytes
    }

    #[test]
    fn converts_and_round_trips_bc3_color_texture() {
        let artifact = TextureArtifact::from_dx10_dds(
            &dds(DXGI_FORMAT_BC3_UNORM_SRGB),
            TextureSemantic::Color,
        )
        .unwrap();
        let encoded = artifact.encode().unwrap();
        assert_eq!(TextureArtifact::decode(&encoded).unwrap(), artifact);
    }

    #[test]
    fn rejects_srgb_normal_texture() {
        assert_eq!(
            TextureArtifact::from_dx10_dds(
                &dds(DXGI_FORMAT_BC3_UNORM_SRGB),
                TextureSemantic::Normal
            ),
            Err(TextureError::SemanticFormatMismatch)
        );
    }

    #[test]
    fn converts_bc1_color_and_bc5_normal_textures() {
        let color = TextureArtifact::from_dx10_dds(
            &dds(DXGI_FORMAT_BC1_UNORM_SRGB),
            TextureSemantic::Color,
        )
        .unwrap();
        assert_eq!(color.descriptor.format, TextureFormat::Bc1UnormSrgb);
        assert_eq!(color.payload.len(), 8);

        let normal =
            TextureArtifact::from_dx10_dds(&dds(DXGI_FORMAT_BC5_UNORM), TextureSemantic::Normal)
                .unwrap();
        assert_eq!(normal.descriptor.format, TextureFormat::Bc5Unorm);
        assert_eq!(normal.payload.len(), 16);
    }

    #[test]
    fn converts_bc4_data_texture() {
        let data =
            TextureArtifact::from_dx10_dds(&dds(DXGI_FORMAT_BC4_UNORM), TextureSemantic::Data)
                .unwrap();
        assert_eq!(data.descriptor.format, TextureFormat::Bc4Unorm);
        assert_eq!(data.payload.len(), 8);
        let encoded = data.encode().unwrap();
        assert_eq!(TextureArtifact::decode(&encoded).unwrap(), data);
    }

    #[test]
    fn converts_bc7_linear_and_srgb_textures() {
        let color = TextureArtifact::from_dx10_dds(
            &dds(DXGI_FORMAT_BC7_UNORM_SRGB),
            TextureSemantic::Color,
        )
        .unwrap();
        assert_eq!(color.descriptor.format, TextureFormat::Bc7UnormSrgb);

        let data =
            TextureArtifact::from_dx10_dds(&dds(DXGI_FORMAT_BC7_UNORM), TextureSemantic::Data)
                .unwrap();
        assert_eq!(data.descriptor.format, TextureFormat::Bc7Unorm);
    }
}
