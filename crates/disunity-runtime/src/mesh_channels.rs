//! Every vertex channel and sub-mesh of the meshes the managed engine gave the host, read from the
//! same native image as [`crate::meshes`]. A shader reads its inputs by channel number.
use crate::{DisunityHost, HostError, native};

/// One vertex channel. `number` is the engine's vertex attribute number: 0 position, 1 normal,
/// 2 tangent, 3 colour (linear RGBA), 4 to 11 texture coordinates 0 to 7, 12 blend weight and
/// 13 blend indices (whole numbers held as floats). `data` holds `dimension` floats for each
/// vertex, in vertex order.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshChannel {
    pub number: u32,
    /// Floats per vertex, 1 to 4.
    pub dimension: u32,
    pub data: Vec<f32>,
}

/// One sub-mesh: `count` indices from `start`, drawn with `topology` (0 triangles, 1 quads,
/// 2 lines, 3 line strip, 4 points). Range `i` is sub-mesh `i`, the material slot, and an empty
/// sub-mesh has a `count` of zero. The indices already include `base_vertex`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshRange {
    pub start: u32,
    pub count: u32,
    pub base_vertex: u32,
    pub topology: u32,
}

/// The channels, indices and sub-meshes of one mesh a player's managed code gave the renderer,
/// beside the fixed vertices of [`crate::PlayerMesh`].
#[derive(Clone, Debug, PartialEq)]
pub struct MeshChannels {
    /// Equals the generation of the [`crate::PlayerMesh`] stored with it.
    pub generation: u64,
    pub vertex_count: u32,
    /// Only the channels the mesh holds, in the order the managed engine stored them.
    pub channels: Vec<MeshChannel>,
    /// The indices of every sub-mesh of any topology, with the base vertex applied.
    pub indices: Vec<u32>,
    pub ranges: Vec<MeshRange>,
}

pub(crate) type Read = unsafe extern "C" fn(
    u64,
    *mut native::DisunityMeshChannelsInfo,
    *mut native::DisunityMeshChannel,
    *mut f32,
    *mut u32,
    *mut native::DisunityMeshRange,
) -> native::DisunityResult;

impl DisunityHost {
    /// The channels stored under mesh `identity`, or `None` when the managed engine stored none.
    pub fn mesh_channels(&self, identity: u64) -> Result<Option<MeshChannels>, HostError> {
        self.mesh_channels_since(identity, 0)
    }

    /// Like [`Self::mesh_channels`], and `None` too when the stored generation is still
    /// `known_generation`, so an unchanged mesh costs no copy.
    pub fn mesh_channels_since(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        // The symbol belongs to the image this host retains.
        let read: Read = unsafe {
            *self
                ._library
                .get(b"disunity_mesh_channels_read\0")
                .map_err(|_| HostError::MissingSymbol("disunity_mesh_channels_read".into()))?
        };
        read_channels(world, read, identity, known_generation, |result| {
            self.check(result)
        })
    }
}

/// Reads the channels of mesh `identity` through `read`, `None` when none are stored or the
/// stored generation is still `known_generation`. `check` turns a failed call into its error.
pub(crate) fn read_channels(
    world: u64,
    read: Read,
    identity: u64,
    known_generation: u64,
    check: impl Fn(native::DisunityResult) -> Result<(), HostError>,
) -> Result<Option<MeshChannels>, HostError> {
    let mut info = native::DisunityMeshChannelsInfo {
        identity,
        generation: known_generation,
        ..Default::default()
    };
    let sizes = unsafe {
        read(
            world,
            &mut info,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    check(sizes)?;
    if info.present == 0 || info.changed == 0 {
        return Ok(None);
    }
    let mut channels = vec![native::DisunityMeshChannel::default(); info.channel_count as usize];
    let mut data = vec![0f32; info.float_count as usize];
    let mut indices = vec![0u32; info.index_count as usize];
    let mut ranges = vec![native::DisunityMeshRange::default(); info.range_count as usize];
    let mut request = info;
    request.generation = known_generation;
    check(unsafe {
        read(
            world,
            &mut request,
            channels.as_mut_ptr(),
            data.as_mut_ptr(),
            indices.as_mut_ptr(),
            ranges.as_mut_ptr(),
        )
    })?;
    Ok(Some(assemble(&info, &channels, &data, indices, &ranges)))
}

fn assemble(
    info: &native::DisunityMeshChannelsInfo,
    channels: &[native::DisunityMeshChannel],
    data: &[f32],
    indices: Vec<u32>,
    ranges: &[native::DisunityMeshRange],
) -> MeshChannels {
    let slice = |channel: &native::DisunityMeshChannel| {
        let start = channel.offset as usize;
        let len = (channel.dimension * info.vertex_count) as usize;
        data[start..start + len].to_vec()
    };
    MeshChannels {
        generation: info.generation,
        vertex_count: info.vertex_count,
        channels: channels
            .iter()
            .map(|channel| MeshChannel {
                number: channel.number,
                dimension: channel.dimension,
                data: slice(channel),
            })
            .collect(),
        indices,
        ranges: ranges
            .iter()
            .map(|range| MeshRange {
                start: range.start,
                count: range.count,
                base_vertex: range.base_vertex,
                topology: range.topology,
            })
            .collect(),
    }
}
