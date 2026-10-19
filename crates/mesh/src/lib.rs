use serde::{Deserialize, Serialize};
use thiserror::Error;
use worldrunner_asset_id::{ContentHash, LogicalAssetId};

const MAGIC: [u8; 8] = *b"RDMESH01";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 52;
const MAX_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeshPrimitive {
    pub first_index: u32,
    pub index_count: u32,
    pub material_slot: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaticMeshArtifact {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub primitives: Vec<MeshPrimitive>,
    pub material_slots: Vec<LogicalAssetId>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Optional two-channel normalized byte colors, parallel to the vertex array.
    pub colors_rg: Option<Vec<[u8; 2]>>,
}

impl StaticMeshArtifact {
    pub fn validate(&self) -> Result<(), MeshError> {
        if self.vertices.is_empty() || self.indices.is_empty() || self.primitives.is_empty() {
            return Err(MeshError::EmptyGeometry);
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err(MeshError::InvalidTriangleList);
        }
        if self.material_slots.is_empty() {
            return Err(MeshError::MissingMaterials);
        }
        if self.vertices.iter().any(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.normal.iter())
                .chain(vertex.tangent.iter())
                .chain(vertex.uv.iter())
                .any(|value| !value.is_finite())
        }) || self
            .bounds_min
            .iter()
            .chain(self.bounds_max.iter())
            .any(|value| !value.is_finite())
        {
            return Err(MeshError::NonFiniteData);
        }
        if self
            .indices
            .iter()
            .any(|&index| usize::try_from(index).map_or(true, |index| index >= self.vertices.len()))
        {
            return Err(MeshError::IndexOutOfBounds);
        }
        self.validate_colors_rg()?;
        let mut expected_first = 0_u32;
        for primitive in &self.primitives {
            if primitive.first_index != expected_first || primitive.index_count % 3 != 0 {
                return Err(MeshError::InvalidPrimitiveRange);
            }
            expected_first = expected_first
                .checked_add(primitive.index_count)
                .ok_or(MeshError::SizeOverflow)?;
            if usize::try_from(primitive.material_slot)
                .map_or(true, |slot| slot >= self.material_slots.len())
            {
                return Err(MeshError::InvalidMaterialSlot);
            }
        }
        if usize::try_from(expected_first).map_err(|_| MeshError::SizeOverflow)?
            != self.indices.len()
        {
            return Err(MeshError::InvalidPrimitiveRange);
        }
        for axis in 0..3 {
            if self.bounds_min[axis] > self.bounds_max[axis] {
                return Err(MeshError::InvalidBounds);
            }
        }
        Ok(())
    }

    fn validate_colors_rg(&self) -> Result<(), MeshError> {
        if let Some(colors_rg) = &self.colors_rg
            && colors_rg.len() != self.vertices.len()
        {
            return Err(MeshError::ColorChannelLengthMismatch);
        }
        Ok(())
    }

    pub fn dependencies(&self) -> Vec<LogicalAssetId> {
        let mut dependencies = self.material_slots.clone();
        dependencies.sort();
        dependencies.dedup();
        dependencies
    }

    pub fn encode(&self) -> Result<Vec<u8>, MeshError> {
        self.validate()?;
        let payload = postcard::to_allocvec(self)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(MeshError::LimitExceeded("artifact payload bytes"));
        }
        let payload_length = u64::try_from(payload.len()).map_err(|_| MeshError::SizeOverflow)?;
        let payload_hash = ContentHash::of(&payload);
        let mut output = Vec::with_capacity(
            HEADER_BYTES
                .checked_add(payload.len())
                .ok_or(MeshError::SizeOverflow)?,
        );
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&VERSION.to_le_bytes());
        output.extend_from_slice(&payload_length.to_le_bytes());
        output.extend_from_slice(&payload_hash.0);
        output.extend_from_slice(&payload);
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MeshError> {
        if bytes.len() < HEADER_BYTES {
            return Err(MeshError::TruncatedArtifact);
        }
        if bytes[..8] != MAGIC {
            return Err(MeshError::InvalidArtifactMagic);
        }
        let version = read_u32_at(bytes, 8).ok_or(MeshError::TruncatedArtifact)?;
        if version != VERSION {
            return Err(MeshError::UnsupportedArtifactVersion(version));
        }
        let payload_length =
            usize::try_from(read_u64_at(bytes, 12).ok_or(MeshError::TruncatedArtifact)?)
                .map_err(|_| MeshError::SizeOverflow)?;
        if payload_length > MAX_PAYLOAD_BYTES {
            return Err(MeshError::LimitExceeded("artifact payload bytes"));
        }
        let payload_end = HEADER_BYTES
            .checked_add(payload_length)
            .ok_or(MeshError::SizeOverflow)?;
        if payload_end != bytes.len() {
            return Err(MeshError::InvalidArtifactExtent);
        }
        let expected_hash = ContentHash(
            bytes[20..52]
                .try_into()
                .map_err(|_| MeshError::TruncatedArtifact)?,
        );
        let payload = &bytes[HEADER_BYTES..payload_end];
        if ContentHash::of(payload) != expected_hash {
            return Err(MeshError::HashMismatch);
        }
        let artifact: Self = postcard::from_bytes(payload)?;
        artifact.validate()?;
        Ok(artifact)
    }
}

#[derive(Debug, Error)]
pub enum MeshError {
    #[error("mesh artifact serialization failed: {0}")]
    Serialization(#[from] postcard::Error),
    #[error("mesh artifact is truncated")]
    TruncatedArtifact,
    #[error("mesh artifact magic is invalid")]
    InvalidArtifactMagic,
    #[error("mesh artifact version {0} is unsupported")]
    UnsupportedArtifactVersion(u32),
    #[error("mesh colors_rg channel length does not match the vertex count")]
    ColorChannelLengthMismatch,
    #[error("mesh artifact extent is invalid")]
    InvalidArtifactExtent,
    #[error("mesh artifact hash does not match")]
    HashMismatch,
    #[error("mesh contains no renderable geometry")]
    EmptyGeometry,
    #[error("mesh indices are not a triangle list")]
    InvalidTriangleList,
    #[error("mesh references no materials")]
    MissingMaterials,
    #[error("mesh contains non-finite values")]
    NonFiniteData,
    #[error("mesh index is out of bounds")]
    IndexOutOfBounds,
    #[error("mesh primitive ranges are invalid")]
    InvalidPrimitiveRange,
    #[error("mesh material slot is invalid")]
    InvalidMaterialSlot,
    #[error("mesh bounds are invalid")]
    InvalidBounds,
    #[error("mesh size arithmetic overflowed")]
    SizeOverflow,
    #[error("mesh limit exceeded: {0}")]
    LimitExceeded(&'static str),
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset.checked_add(4)?)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

fn read_u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    bytes
        .get(offset..offset.checked_add(8)?)?
        .try_into()
        .ok()
        .map(u64::from_le_bytes)
}
