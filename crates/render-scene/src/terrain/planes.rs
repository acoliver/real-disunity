//! Rasterized triangle reconstruction without changing mesh vertex sharing.
//!
//! Group 1 binding 23 is a read-only u32 array. Its four-word header points to
//! east coordinates, north coordinates, heights (all exact mesh f32 bits), and
//! eight-word chunk descriptors: minimum XY, maximum XY, stride, width, height,
//! and one reserved zero. Instance lanes carry the descriptor index as two u16s.
use super::streaming::TerrainChunk;
use super::{axis_positions, lod_stride};
use crate::geometry::millimeter_to_meter;
use std::collections::BTreeMap;
use worldrunner_render_plan::SceneTerrain;

/// Exact GPU-boundary positions and the sample windows each draw can select.
#[derive(Debug)]
pub(crate) struct HeightfieldPlanes {
    /// Encoded header, source coordinates and chunk descriptors.
    pub bytes: Vec<u8>,
    /// Stable descriptor indices shared by instance upload and streaming draws.
    pub chunks: BTreeMap<TerrainChunk, u32>,
}

/// Read-only exact triangle source in the terrain bind group.
pub(crate) const BINDING: u32 = 23;

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
) -> Result<wgpu::Buffer, crate::draw::DrawError> {
    validate_size(bytes.len() as u64, &device.limits())?;
    Ok(crate::upload::upload_buffer(
        device,
        queue,
        "terrain triangle source",
        wgpu::BufferUsages::STORAGE,
        bytes,
    ))
}

fn validate_size(required: u64, limits: &wgpu::Limits) -> Result<(), crate::draw::DrawError> {
    let limit = limits
        .max_buffer_size
        .min(limits.max_storage_buffer_binding_size);
    if required > limit {
        return Err(crate::draw::DrawError::TerrainPlaneLimit { required, limit });
    }
    Ok(())
}

pub(crate) fn instance_size(
    count: usize,
    limits: &wgpu::Limits,
) -> Result<usize, crate::draw::DrawError> {
    use crate::draw::DrawError;
    let bytes = count
        .checked_mul(crate::geometry::INSTANCE_BYTES)
        .ok_or(DrawError::TerrainInstanceSizeOverflow)?;
    let required = u64::try_from(bytes).map_err(|_| DrawError::TerrainInstanceSizeOverflow)?;
    let limit = limits.max_buffer_size;
    if required > limit {
        return Err(DrawError::TerrainInstanceLimit { required, limit });
    }
    Ok(bytes)
}

impl HeightfieldPlanes {
    pub fn new(terrain: &SceneTerrain) -> Self {
        let mut nodes = Vec::new();
        collect_chunks(
            root(terrain),
            terrain.streaming_policy().is_some(),
            &mut nodes,
        );
        let columns = terrain.columns();
        let rows = terrain.rows();
        let offsets = [
            4,
            4 + columns,
            4 + columns + rows,
            4 + columns + rows + columns * rows,
        ];
        let mut bytes: Vec<_> = offsets.into_iter().flat_map(u32::to_le_bytes).collect();
        for (origin, count) in [
            (terrain.origin_east_millimeters(), columns),
            (terrain.origin_north_millimeters(), rows),
        ] {
            let samples: Vec<_> = (0..count).collect();
            for position in axis_positions(origin, terrain.sample_spacing_millimeters(), &samples)
                .expect("validated terrain extent")
            {
                bytes.extend_from_slice(&millimeter_to_meter(position).to_le_bytes());
            }
        }
        for height in terrain.heights_millimeters() {
            bytes.extend_from_slice(&millimeter_to_meter(i64::from(*height)).to_le_bytes());
        }
        let mut chunks = BTreeMap::new();
        for (index, chunk) in nodes.into_iter().enumerate() {
            chunks.insert(chunk, u32::try_from(index).expect("addressable chunks"));
            let width = (chunk.maximum[0] - chunk.minimum[0]).div_ceil(chunk.stride) + 1;
            let height = (chunk.maximum[1] - chunk.minimum[1]).div_ceil(chunk.stride) + 1;
            for word in [
                chunk.minimum[0],
                chunk.minimum[1],
                chunk.maximum[0],
                chunk.maximum[1],
                chunk.stride,
                width,
                height,
                0,
            ] {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        Self { bytes, chunks }
    }
}

pub(crate) fn chunk_indices(terrain: &SceneTerrain) -> BTreeMap<TerrainChunk, u32> {
    let mut nodes = Vec::new();
    collect_chunks(
        root(terrain),
        terrain.streaming_policy().is_some(),
        &mut nodes,
    );
    nodes
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| (chunk, u32::try_from(index).expect("addressable chunks")))
        .collect()
}

pub(crate) fn root(terrain: &SceneTerrain) -> TerrainChunk {
    terrain.streaming_policy().map_or_else(
        || TerrainChunk {
            minimum: [0; 2],
            maximum: [terrain.columns() - 1, terrain.rows() - 1],
            stride: lod_stride(terrain.lod_policy()),
        },
        |policy| TerrainChunk::root(terrain, policy),
    )
}

fn collect_chunks(chunk: TerrainChunk, streaming: bool, nodes: &mut Vec<TerrainChunk>) {
    nodes.push(chunk);
    if streaming && let Some(children) = chunk.children() {
        for child in children {
            collect_chunks(child, true, nodes);
        }
    }
}

pub(crate) fn instance_bytes(index: u32) -> Vec<u8> {
    let mut bytes = crate::geometry::pack_instance(&super::TERRAIN_PLACEMENT);
    bytes[64..68].copy_from_slice(
        &f32::from(u16::try_from(index & 0xffff).expect("low half")).to_le_bytes(),
    );
    bytes[68..72]
        .copy_from_slice(&f32::from(u16::try_from(index >> 16).expect("high half")).to_le_bytes());
    bytes
}

#[cfg(test)]
pub(crate) mod tests;
