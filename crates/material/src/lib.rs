use serde::{Deserialize, Serialize};
use thiserror::Error;
use worldrunner_asset_id::{ContentHash, LogicalAssetId};

const MAGIC: [u8; 8] = *b"RDMATL01";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 52;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialArtifact {
    pub albedo_texture: LogicalAssetId,
    pub normal_texture: Option<LogicalAssetId>,
    #[serde(alias = "orm_texture")]
    pub roughness_metallic_texture: Option<LogicalAssetId>,
    #[serde(default)]
    pub ambient_occlusion_texture: Option<LogicalAssetId>,
    #[serde(default)]
    pub self_illumination_texture: Option<LogicalAssetId>,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub normal_scale: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// Interpret the auxiliary surface mask as foliage translucency.
    #[serde(default)]
    pub translucent_foliage: bool,
    /// Positive per-axis texture coordinate multiplier.
    #[serde(default = "default_uv_tiling")]
    pub uv_tiling: [f32; 2],
    /// Blend shading normals toward the terrain normal when requested.
    #[serde(default)]
    pub terrain_distorted: bool,
}

fn default_uv_tiling() -> [f32; 2] {
    [1.0, 1.0]
}

impl MaterialArtifact {
    pub fn validate(&self) -> Result<(), MaterialError> {
        if self
            .base_color_factor
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(MaterialError::InvalidBaseColor);
        }
        if !in_unit_range(self.metallic_factor) || !in_unit_range(self.roughness_factor) {
            return Err(MaterialError::InvalidPbrFactor);
        }
        if !self.normal_scale.is_finite() || !(0.0..=4.0).contains(&self.normal_scale) {
            return Err(MaterialError::InvalidNormalScale);
        }
        if !in_unit_range(self.alpha_cutoff) {
            return Err(MaterialError::InvalidAlphaCutoff);
        }
        if self
            .uv_tiling
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0 || *value > 64.0)
        {
            return Err(MaterialError::InvalidUvTiling);
        }
        Ok(())
    }

    pub fn dependencies(&self) -> Vec<LogicalAssetId> {
        let mut dependencies = vec![self.albedo_texture.clone()];
        if let Some(normal) = &self.normal_texture {
            dependencies.push(normal.clone());
        }
        if let Some(roughness_metallic) = &self.roughness_metallic_texture {
            dependencies.push(roughness_metallic.clone());
        }
        if let Some(ambient_occlusion) = &self.ambient_occlusion_texture {
            dependencies.push(ambient_occlusion.clone());
        }
        if let Some(self_illumination) = &self.self_illumination_texture {
            dependencies.push(self_illumination.clone());
        }
        dependencies.sort();
        dependencies.dedup();
        dependencies
    }

    pub fn encode(&self) -> Result<Vec<u8>, MaterialError> {
        self.validate()?;
        let payload = postcard::to_allocvec(self)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(MaterialError::PayloadLimitExceeded);
        }
        let payload_length =
            u64::try_from(payload.len()).map_err(|_| MaterialError::SizeOverflow)?;
        let payload_hash = ContentHash::of(&payload);
        let mut output = Vec::with_capacity(
            HEADER_BYTES
                .checked_add(payload.len())
                .ok_or(MaterialError::SizeOverflow)?,
        );
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&VERSION.to_le_bytes());
        output.extend_from_slice(&payload_length.to_le_bytes());
        output.extend_from_slice(&payload_hash.0);
        output.extend_from_slice(&payload);
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MaterialError> {
        if bytes.len() < HEADER_BYTES {
            return Err(MaterialError::Truncated);
        }
        if bytes[..8] != MAGIC {
            return Err(MaterialError::InvalidMagic);
        }
        let version = read_u32(bytes, 8)?;
        if version != VERSION {
            return Err(MaterialError::UnsupportedVersion(version));
        }
        let payload_length =
            usize::try_from(read_u64(bytes, 12)?).map_err(|_| MaterialError::SizeOverflow)?;
        if payload_length > MAX_PAYLOAD_BYTES {
            return Err(MaterialError::PayloadLimitExceeded);
        }
        let payload_end = HEADER_BYTES
            .checked_add(payload_length)
            .ok_or(MaterialError::SizeOverflow)?;
        if payload_end != bytes.len() {
            return Err(MaterialError::InvalidExtent);
        }
        let expected_hash = ContentHash(
            bytes[20..52]
                .try_into()
                .map_err(|_| MaterialError::Truncated)?,
        );
        let payload = &bytes[HEADER_BYTES..payload_end];
        if ContentHash::of(payload) != expected_hash {
            return Err(MaterialError::HashMismatch);
        }
        let material: Self = postcard::from_bytes(payload)?;
        material.validate()?;
        Ok(material)
    }
}

fn in_unit_range(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MaterialError> {
    bytes
        .get(offset..offset + 4)
        .ok_or(MaterialError::Truncated)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| MaterialError::Truncated)
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, MaterialError> {
    bytes
        .get(offset..offset + 8)
        .ok_or(MaterialError::Truncated)?
        .try_into()
        .map(u64::from_le_bytes)
        .map_err(|_| MaterialError::Truncated)
}

#[derive(Debug, Error)]
pub enum MaterialError {
    #[error("material serialization failed: {0}")]
    Serialization(#[from] postcard::Error),
    #[error("material artifact is truncated")]
    Truncated,
    #[error("material artifact magic is invalid")]
    InvalidMagic,
    #[error("material artifact version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("material artifact extent is invalid")]
    InvalidExtent,
    #[error("material payload exceeds the configured limit")]
    PayloadLimitExceeded,
    #[error("material payload hash does not match")]
    HashMismatch,
    #[error("material size arithmetic overflowed")]
    SizeOverflow,
    #[error("material base color factor must be finite and between zero and one")]
    InvalidBaseColor,
    #[error("material metallic and roughness factors must be finite and between zero and one")]
    InvalidPbrFactor,
    #[error("material normal scale must be finite and between zero and four")]
    InvalidNormalScale,
    #[error("material uv tiling must be finite, positive, and at most 64")]
    InvalidUvTiling,
    #[error("material alpha cutoff must be finite and between zero and one")]
    InvalidAlphaCutoff,
}
