//! Every vertex channel and every sub-mesh topology of a mesh the managed engine hands to the host,
//! kept beside the fixed vertices of [`crate::meshes`]. A shader reads its inputs by channel
//! number, so the host reads the channels from here.
use crate::{
    DisunityResult,
    callbacks::mutate,
    meshes::{MAX_INDICES, MAX_SUBMESHES, copy_records, records},
};

/// The engine's vertex channels: 0 position, 1 normal, 2 tangent, 3 colour, 4 to 11 texture
/// coordinates 0 to 7, 12 blend weight, 13 blend indices.
pub const CHANNEL_NUMBERS: u32 = 14;
/// The largest dimension of one channel.
const MAX_DIMENSION: u32 = 4;
/// The most floats one stored mesh holds across its channels.
pub(crate) const MAX_FLOATS: u32 = 1 << 28;
/// Topology values: 0 triangles, 1 quads, 2 lines, 3 line strip, 4 points.
pub const TOPOLOGY_COUNT: u32 = 5;

/// A stored mesh's channels, also the in/out record of a read. On a store, `identity` names the
/// mesh, `vertex_count` must equal the fixed mesh's, the four counts are those of the buffers
/// passed, and `generation`, `present`, `changed` and `copied` are ignored. On a read,
/// `identity` names the mesh, `generation` is the generation the caller already holds and the
/// four counts are the capacities of the buffers passed (all zero asks for sizes only); the call
/// fills in everything.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshChannelsInfo {
    pub identity: u64,
    /// The generation of the fixed mesh the channels were stored for, or one more than the held
    /// channels' when they are stored again without the fixed mesh changing.
    pub generation: u64,
    /// Zero when a read names an identity with no stored channels.
    pub present: u32,
    /// Nonzero when the stored generation is newer than the one the read passed.
    pub changed: u32,
    /// Nonzero when the read copied the buffers.
    pub copied: u32,
    pub vertex_count: u32,
    pub channel_count: u32,
    pub float_count: u32,
    pub index_count: u32,
    pub range_count: u32,
}

/// One channel: its number, its float count per vertex (1 to 4) and the offset in floats of its
/// first value in the shared float buffer. Vertex `v` holds `dimension` floats from
/// `offset + v * dimension`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshChannel {
    pub number: u32,
    pub dimension: u32,
    pub offset: u32,
}

/// A run of indices that forms one sub-mesh, drawn with the given topology: 0 triangles, 1 quads,
/// 2 lines, 3 line strip, 4 points. Range `i` is sub-mesh `i` (its material slot), so an empty
/// sub-mesh is a range with `count` zero. `base_vertex` is the offset the managed mesh adds to the
/// sub-mesh's indices; the stored indices already include it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityMeshRange {
    pub start: u32,
    pub count: u32,
    pub base_vertex: u32,
    pub topology: u32,
}

/// The channels of one stored mesh.
pub(crate) struct StoredChannels {
    info: DisunityMeshChannelsInfo,
    channels: Vec<DisunityMeshChannel>,
    data: Vec<f32>,
    indices: Vec<u32>,
    ranges: Vec<DisunityMeshRange>,
}

impl StoredChannels {
    pub(crate) fn generation(&self) -> u64 {
        self.info.generation
    }
}

/// The buffers of one store call, read from the caller's pointers.
struct Parts<'a> {
    info: DisunityMeshChannelsInfo,
    channels: &'a [DisunityMeshChannel],
    data: &'a [f32],
    indices: &'a [u32],
    ranges: &'a [DisunityMeshRange],
}

fn check_channels(parts: &Parts) -> Result<(), &'static str> {
    let mut seen = 0u32;
    for channel in parts.channels {
        if channel.number >= CHANNEL_NUMBERS {
            return Err("a mesh channel number is beyond the engine's channels");
        }
        let bit = 1u32 << channel.number;
        if seen & bit != 0 {
            return Err("a mesh channel is stored twice");
        }
        seen |= bit;
        if channel.dimension == 0 || channel.dimension > MAX_DIMENSION {
            return Err("a mesh channel needs one to four floats per vertex");
        }
        let end = u64::from(channel.offset)
            + u64::from(channel.dimension) * u64::from(parts.info.vertex_count);
        if end > parts.data.len() as u64 {
            return Err("a mesh channel reaches past the float data");
        }
    }
    Ok(())
}

fn check_ranges(parts: &Parts) -> Result<(), &'static str> {
    for range in parts.ranges {
        let end = u64::from(range.start) + u64::from(range.count);
        if end > parts.indices.len() as u64 {
            return Err("a mesh range reaches past the indices");
        }
        if range.topology >= TOPOLOGY_COUNT {
            return Err("a mesh range has a topology the engine does not have");
        }
    }
    if parts.indices.iter().any(|i| *i >= parts.info.vertex_count) {
        return Err("a mesh index is past the vertices");
    }
    Ok(())
}

fn check_parts(parts: &Parts) -> Result<(), &'static str> {
    if parts.data.iter().any(|value| !value.is_finite()) {
        return Err("a mesh channel holds a value that is not finite");
    }
    check_channels(parts)?;
    check_ranges(parts)
}

/// Keeps a mesh's channels, every sub-mesh and all its indices beside the fixed mesh stored under
/// the same identity. Storing the fixed mesh again drops the channels, so store them after it.
///
/// # Safety
/// `info` is readable for one record. `channels`, `data`, `indices` and `ranges` are readable for
/// the counts in the record, or null when that count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_channels_store(
    world: u64,
    info: *const DisunityMeshChannelsInfo,
    channels: *const DisunityMeshChannel,
    data: *const f32,
    indices: *const u32,
    ranges: *const DisunityMeshRange,
) -> DisunityResult {
    if info.is_null() {
        return DisunityResult::invalid_argument();
    }
    let info = unsafe { *info };
    if info.channel_count > CHANNEL_NUMBERS
        || info.float_count > MAX_FLOATS
        || info.index_count > MAX_INDICES
        || info.range_count > MAX_SUBMESHES
    {
        return DisunityResult::invalid_argument();
    }
    let (Some(channels), Some(data), Some(indices), Some(ranges)) = (
        unsafe { records(channels, info.channel_count) },
        unsafe { records(data, info.float_count) },
        unsafe { records(indices, info.index_count) },
        unsafe { records(ranges, info.range_count) },
    ) else {
        return DisunityResult::invalid_argument();
    };
    let parts = Parts {
        info,
        channels,
        data,
        indices,
        ranges,
    };
    mutate(world, |s| {
        let Some(mesh) = s.meshes.get_mut(&info.identity) else {
            return Err("channels need the fixed mesh stored under the same identity first".into());
        };
        if mesh.vertex_count() != info.vertex_count {
            return Err("the channels disagree with the fixed mesh's vertex count".into());
        }
        check_parts(&parts)?;
        let next = mesh
            .channels
            .as_ref()
            .map_or(0, |held| held.generation() + 1);
        let stored = DisunityMeshChannelsInfo {
            generation: mesh.generation().max(next),
            present: 1,
            changed: 0,
            copied: 0,
            ..info
        };
        mesh.channels = Some(StoredChannels {
            info: stored,
            channels: channels.to_vec(),
            data: data.to_vec(),
            indices: indices.to_vec(),
            ranges: ranges.to_vec(),
        });
        Ok(())
    })
}

/// Buffers of a read, as raw pointers the caller sized from the capacities in the record.
struct Targets {
    channels: *mut DisunityMeshChannel,
    data: *mut f32,
    indices: *mut u32,
    ranges: *mut DisunityMeshRange,
}

/// Reports a mesh's stored channels and copies them when they are newer than `info.generation`
/// and the capacities in `info` hold them. Capacities of zero everywhere copy nothing and report
/// the sizes. An identity with no stored channels reports `present` zero and is not an error.
///
/// # Safety
/// `info` is readable and writable for one record; the buffers are writable for the capacities
/// in it, or null when that capacity is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_mesh_channels_read(
    world: u64,
    info: *mut DisunityMeshChannelsInfo,
    channels: *mut DisunityMeshChannel,
    data: *mut f32,
    indices: *mut u32,
    ranges: *mut DisunityMeshRange,
) -> DisunityResult {
    if info.is_null() {
        return DisunityResult::invalid_argument();
    }
    let request = unsafe { *info };
    let capacities = [
        request.channel_count,
        request.float_count,
        request.index_count,
        request.range_count,
    ];
    let targets = Targets {
        channels,
        data,
        indices,
        ranges,
    };
    if !targets.hold(capacities) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let held = s
            .meshes
            .get(&request.identity)
            .and_then(|mesh| mesh.channels.as_ref());
        let Some(held) = held else {
            unsafe {
                *info = DisunityMeshChannelsInfo {
                    identity: request.identity,
                    ..Default::default()
                };
            }
            return Ok(());
        };
        let changed = held.info.generation > request.generation;
        let copy = changed && capacities.iter().any(|c| *c != 0);
        if copy && !fits(held, capacities) {
            return Err("mesh channel copy capacity is too small".into());
        }
        unsafe {
            *info = DisunityMeshChannelsInfo {
                changed: u32::from(changed),
                copied: u32::from(copy),
                ..held.info
            };
            if copy {
                targets.copy_from(held);
            }
        }
        Ok(())
    })
}

impl Targets {
    /// Whether every buffer with a nonzero capacity was passed.
    fn hold(&self, capacities: [u32; 4]) -> bool {
        let nulls = [
            self.channels.is_null(),
            self.data.is_null(),
            self.indices.is_null(),
            self.ranges.is_null(),
        ];
        capacities
            .iter()
            .zip(nulls)
            .all(|(capacity, null)| *capacity == 0 || !null)
    }

    /// # Safety
    /// Each buffer is writable for the stored counts of `held`, or null when that count is zero.
    unsafe fn copy_from(&self, held: &StoredChannels) {
        unsafe {
            copy_records(&held.channels, self.channels);
            copy_records(&held.data, self.data);
            copy_records(&held.indices, self.indices);
            copy_records(&held.ranges, self.ranges);
        }
    }
}

fn fits(held: &StoredChannels, [channels, floats, indices, ranges]: [u32; 4]) -> bool {
    let info = held.info;
    channels >= info.channel_count
        && floats >= info.float_count
        && indices >= info.index_count
        && ranges >= info.range_count
}

#[cfg(test)]
#[path = "mesh_channel_tests.rs"]
mod tests;
