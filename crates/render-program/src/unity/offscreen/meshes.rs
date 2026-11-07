//! Meshes: every vertex channel and the indices of a mesh live in one buffer of the runtime, and a
//! draw binds a byte range of it for each channel.

use std::collections::HashMap;
use std::ops::Range;

use crate::error::ProgramError;
use crate::ids::MeshId;
use crate::resources::BufferHandle;
use crate::runtime::ProgramRuntime;

use super::error::OffscreenError;
use super::frame::{ChannelFormat, Mesh, MeshIndices, SubMesh};

/// Where one channel's bytes are in the mesh's buffer.
pub(super) struct ChannelInfo {
    pub(super) source: i8,
    pub(super) format: ChannelFormat,
    pub(super) range: Range<u64>,
}

pub(super) struct IndexInfo {
    pub(super) range: Range<u64>,
    pub(super) format: wgpu::IndexFormat,
}

/// The channel a player's vertex tables number 0, which holds the positions.
const POSITION_CHANNEL: i8 = 0;

pub(super) struct MeshInfo {
    pub(super) buffer: BufferHandle,
    pub(super) vertex_count: u32,
    /// The positions as x, y, z triples, kept on the CPU for the probe of a camera render.
    pub(super) positions: Option<Vec<f32>>,
    pub(super) channels: Vec<ChannelInfo>,
    pub(super) index: Option<IndexInfo>,
    pub(super) sub_meshes: Vec<SubMesh>,
}

impl MeshInfo {
    pub(super) fn channel(&self, source: i8) -> Option<&ChannelInfo> {
        self.channels
            .iter()
            .find(|channel| channel.source == source)
    }
}

#[derive(Default)]
pub(super) struct MeshTable {
    meshes: HashMap<MeshId, MeshInfo>,
    retired: Vec<BufferHandle>,
}

impl MeshTable {
    pub(super) fn get(&self, id: MeshId) -> Result<&MeshInfo, OffscreenError> {
        self.meshes.get(&id).ok_or(OffscreenError::UnknownMesh(id))
    }

    /// Uploads a mesh, replacing the one of that id.
    pub(super) fn set(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: MeshId,
        mesh: &Mesh,
    ) -> Result<(), OffscreenError> {
        let bad = |reason: String| OffscreenError::BadMesh { mesh: id, reason };
        check(mesh).map_err(bad)?;
        let mut bytes = Vec::new();
        let mut channels = Vec::new();
        for channel in &mesh.channels {
            let start = place(&mut bytes);
            bytes.extend(&channel.bytes);
            channels.push(ChannelInfo {
                source: channel.source,
                format: channel.format,
                range: start..bytes.len() as u64,
            });
        }
        let positions = mesh
            .channels
            .iter()
            .find(|channel| channel.source == POSITION_CHANNEL)
            .filter(|channel| channel.format == ChannelFormat::Float32x3)
            .map(|channel| {
                channel
                    .bytes
                    .chunks_exact(4)
                    .map(|float| f32::from_le_bytes([float[0], float[1], float[2], float[3]]))
                    .collect()
            });
        let index = write_indices(&mut bytes, &mesh.indices);
        let buffer = runtime.create_buffer("offscreen mesh", &bytes)?;
        let info = MeshInfo {
            buffer,
            vertex_count: mesh.vertex_count,
            positions,
            channels,
            index,
            sub_meshes: mesh.sub_meshes.clone(),
        };
        if let Some(old) = self.meshes.insert(id, info) {
            self.retired.push(old.buffer);
        }
        self.retired
            .retain(|buffer| matches!(runtime.release_buffer(*buffer), Err(ProgramError::Busy)));
        Ok(())
    }
}

/// Pads `bytes` to a multiple of four and returns the length, where the next piece starts.
fn place(bytes: &mut Vec<u8>) -> u64 {
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes.len() as u64
}

fn write_indices(bytes: &mut Vec<u8>, indices: &MeshIndices) -> Option<IndexInfo> {
    let start = place(bytes);
    let format = match indices {
        MeshIndices::None => return None,
        MeshIndices::U16(values) => {
            bytes.extend(values.iter().flat_map(|index| index.to_le_bytes()));
            wgpu::IndexFormat::Uint16
        }
        MeshIndices::U32(values) => {
            bytes.extend(values.iter().flat_map(|index| index.to_le_bytes()));
            wgpu::IndexFormat::Uint32
        }
    };
    Some(IndexInfo {
        range: start..bytes.len() as u64,
        format,
    })
}

/// Checks the sizes the mesh states against its data.
fn check(mesh: &Mesh) -> Result<(), String> {
    if mesh.vertex_count == 0 {
        return Err("it has no vertices".to_owned());
    }
    for channel in &mesh.channels {
        let needed = u64::from(mesh.vertex_count) * channel.format.stride();
        if channel.bytes.len() as u64 != needed {
            return Err(format!(
                "channel {} has {} bytes, {} vertices of {:?} need {needed}",
                channel.source,
                channel.bytes.len(),
                mesh.vertex_count,
                channel.format
            ));
        }
    }
    let limit = match &mesh.indices {
        MeshIndices::None => mesh.vertex_count as usize,
        MeshIndices::U16(values) => values.len(),
        MeshIndices::U32(values) => values.len(),
    };
    for (index, sub) in mesh.sub_meshes.iter().enumerate() {
        let end = u64::from(sub.first) + u64::from(sub.count);
        if end > limit as u64 {
            return Err(format!(
                "sub-mesh {index} reaches {end}, past the {limit} indices or vertices it reads"
            ));
        }
    }
    Ok(())
}
