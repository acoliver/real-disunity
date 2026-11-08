//! The meshes and textures the managed engine stored, as render-program's meshes and source
//! textures.
//!
//! A mesh keeps the vertex channels the managed engine stored it with, under their own numbers,
//! which are the numbers the shaders' vertex input tables use: 0 position, 1 normal, 2 tangent,
//! 3 colour, 4 to 11 the texture coordinate sets 0 to 7. Blend weights and indices (12 and 13)
//! are not inputs of this renderer and are left out.

use worldrunner_disunity_runtime::{
    DisunityHost, GpuFlush, MeshChannels, PlayerTexture, TextureSampler,
};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, MeshIndices, SourceTexture, SubMesh,
};
use worldrunner_world_simulation::MAX_RENDERER_MATERIALS;
use worldrunner_world_simulation::graphics::WorldSampler;

use crate::textures::{PlayerTextureError, player_rgba8};

/// The first channel number that is not a vertex input here: blend weights.
const FIRST_SKIN_CHANNEL: u32 = 12;
/// The vertex channel numbers of the positions and of the first texture coordinate set.
pub const POSITION: i8 = 0;
pub const TEXTURE_COORDINATES: i8 = 4;

/// `GraphicsFormat` codes of 8-bit RGBA, in linear and in sRGB.
const RGBA8_LINEAR: u32 = 8;
/// The engine's `TextureFormat` of BC6H, and the `GraphicsFormat` of its unsigned-float form,
/// which the managed engine maps it to. High dynamic range data does not fit RGBA8, so the
/// blocks go to the device as they are.
const BC6H: u32 = 24;
const BC6H_UFLOAT: u32 = 106;
const RGBA8_SRGB: u32 = 4;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AssetError(pub String);

/// Where the host reads what the managed engine stored.
pub trait Assets {
    /// The channels, indices and sub-meshes stored under `identity`, or `None` when none are
    /// stored or their generation is still `known_generation`.
    fn mesh(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, AssetError>;

    /// The texture stored under `identity`, or `None` when none is stored or its generation is
    /// still `known_generation`.
    fn texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, AssetError>;

    /// How the texture stored under `identity` is sampled, or `None` when none is stored.
    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError>;
}

impl Assets for DisunityHost {
    fn mesh(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, AssetError> {
        self.mesh_channels_since(identity, known_generation)
            .map_err(|error| AssetError(error.to_string()))
    }

    fn texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, AssetError> {
        self.player_texture(identity, known_generation)
            .map_err(|error| AssetError(error.to_string()))
    }

    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError> {
        self.texture_sampler(identity)
            .map_err(|error| AssetError(error.to_string()))
    }
}

/// A flush reads what the engine stored the way the host does between frames, so a mesh or
/// texture it names for the first time is there for that flush.
impl Assets for GpuFlush<'_> {
    fn mesh(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, AssetError> {
        self.mesh_channels_since(identity, known_generation)
            .map_err(|error| AssetError(error.to_string()))
    }

    fn texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, AssetError> {
        self.player_texture(identity, known_generation)
            .map_err(|error| AssetError(error.to_string()))
    }

    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError> {
        self.texture_sampler(identity)
            .map_err(|error| AssetError(error.to_string()))
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeshError {
    #[error("the mesh has no triangles")]
    NoTriangles,
    #[error("the mesh has no position channel")]
    NoPositions,
    #[error("channel {number} has {found} floats for {expected}")]
    ChannelLength {
        number: u32,
        found: usize,
        expected: usize,
    },
    #[error("channel {number} has {dimension} floats a vertex, not 1 to 4")]
    Dimension { number: u32, dimension: u32 },
    #[error("the mesh has {0} sub-meshes, past the {MAX_SLOTS} a renderer holds")]
    SubMeshSlot(usize),
    #[error("a sub-mesh is drawn as quads, which this renderer does not draw")]
    QuadTopology,
    #[error("a sub-mesh is drawn as lines, which this renderer does not draw")]
    LineTopology,
    #[error("a sub-mesh is drawn as a line strip, which this renderer does not draw")]
    LineStripTopology,
    #[error("a sub-mesh is drawn as points, which this renderer does not draw")]
    PointTopology,
    #[error("a sub-mesh has topology {0}, which the engine does not define")]
    UnknownTopology(u32),
}

/// Most material slots, and so most sub-meshes, the world gives a renderer.
const MAX_SLOTS: usize = MAX_RENDERER_MATERIALS as usize;

fn format_of(number: u32, dimension: u32) -> Result<ChannelFormat, MeshError> {
    match dimension {
        1 => Ok(ChannelFormat::Float32),
        2 => Ok(ChannelFormat::Float32x2),
        3 => Ok(ChannelFormat::Float32x3),
        4 => Ok(ChannelFormat::Float32x4),
        _ => Err(MeshError::Dimension { number, dimension }),
    }
}

/// The sub-meshes by range number, which is the material slot. A range with no indices stays as
/// a gap that draws nothing. The managed engine has applied each range's base vertex to its
/// indices, so the sub-mesh adds none. A range that draws is refused unless it is triangles.
fn sub_meshes(mesh: &MeshChannels) -> Result<Vec<SubMesh>, MeshError> {
    if mesh.ranges.len() > MAX_SLOTS {
        return Err(MeshError::SubMeshSlot(mesh.ranges.len()));
    }
    mesh.ranges
        .iter()
        .map(|range| {
            if range.count > 0 {
                match range.topology {
                    0 => {}
                    1 => return Err(MeshError::QuadTopology),
                    2 => return Err(MeshError::LineTopology),
                    3 => return Err(MeshError::LineStripTopology),
                    4 => return Err(MeshError::PointTopology),
                    other => return Err(MeshError::UnknownTopology(other)),
                }
            }
            Ok(SubMesh {
                first: range.start,
                count: range.count,
                base_vertex: 0,
            })
        })
        .collect()
}

/// The stored channels of a mesh as a render-program mesh.
///
/// # Errors
/// Fails when the mesh has no position channel or no triangles, a channel does not have
/// `dimension` floats for each vertex, a range that draws is not triangles, or the mesh has more
/// sub-meshes than a renderer has slots.
pub fn mesh_of(stored: &MeshChannels) -> Result<Mesh, MeshError> {
    let vertices = stored.vertex_count as usize;
    if stored.indices.is_empty() || stored.ranges.iter().all(|range| range.count == 0) {
        return Err(MeshError::NoTriangles);
    }
    if !stored
        .channels
        .iter()
        .any(|channel| channel.number == u32::from(POSITION.unsigned_abs()))
    {
        return Err(MeshError::NoPositions);
    }
    let mut channels = Vec::new();
    for channel in &stored.channels {
        if channel.number >= FIRST_SKIN_CHANNEL {
            continue;
        }
        let format = format_of(channel.number, channel.dimension)?;
        let expected = vertices * channel.dimension as usize;
        if channel.data.len() != expected {
            return Err(MeshError::ChannelLength {
                number: channel.number,
                found: channel.data.len(),
                expected,
            });
        }
        channels.push(MeshChannel {
            source: i8::try_from(channel.number).unwrap_or(i8::MAX),
            format,
            bytes: channel
                .data
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect(),
        });
    }
    Ok(Mesh {
        vertex_count: stored.vertex_count,
        channels,
        indices: MeshIndices::U32(stored.indices.clone()),
        sub_meshes: sub_meshes(stored)?,
    })
}

fn wrap_of(mode: u32) -> Wrap {
    match mode {
        1 => Wrap::Clamp,
        // Mirror once repeats the mirrored image once and then clamps; render-program has no such
        // mode, and the mirror matches it inside one mirrored tile.
        2 | 3 => Wrap::Mirror,
        _ => Wrap::Repeat,
    }
}

fn filter_of(mode: u32) -> Filter {
    match mode {
        0 => Filter::Point,
        2 => Filter::Trilinear,
        _ => Filter::Bilinear,
    }
}

/// The wrap and filter of a stored texture's sampler. The wrap along w, anisotropy and mip bias
/// are not carried by render-program's sampler state. A value the engine does not define reads
/// as the engine's default: repeat and bilinear.
#[must_use]
pub fn sampler_of_stored(sampler: &TextureSampler) -> SamplerState {
    SamplerState {
        wrap_u: wrap_of(sampler.wrap_u),
        wrap_v: wrap_of(sampler.wrap_v),
        filter: filter_of(sampler.filter),
    }
}

/// The wrap and filter of a render texture's sampler; see [`sampler_of_stored`].
#[must_use]
pub fn sampler_of_world(sampler: &WorldSampler) -> SamplerState {
    SamplerState {
        wrap_u: wrap_of(sampler.wrap_u),
        wrap_v: wrap_of(sampler.wrap_v),
        filter: filter_of(sampler.filter),
    }
}

/// The `GraphicsFormat` of a stored block-compressed texture's blocks and the bytes per 4x4 block,
/// for the engine `TextureFormat`s the device samples as they are: DXT1 (10), DXT5 (12), BC7 (25)
/// and BC4 (26). They follow the texture's colour space. BC4 has no sRGB form, so a BC4 texture
/// that is not linear is decoded like the decoded path reads it. BC6H (24) is handled apart
/// because it is always sent as blocks.
fn block_upload(stored: &PlayerTexture) -> Option<(u32, usize, &'static str)> {
    let (srgb, linear, bytes, name) = match stored.format {
        10 => (96, 97, 8, "DXT1"),
        12 => (100, 101, 16, "DXT5"),
        25 => (108, 109, 16, "BC7"),
        26 if stored.linear => (102, 102, 8, "BC4"),
        _ => return None,
    };
    Some((if stored.linear { linear } else { srgb }, bytes, name))
}

/// A stored texture's base level as a source texture. Its rows stay bottom first, as the engine
/// stores them, which is the orientation of a render texture's rows (see
/// `camera::clip_from_gl`). It is read with `sampler`, or the fixed sampler without one.
///
/// With `blocks`, a DXT1, DXT5, BC4 or BC7 texture whose sides are multiples of four goes to the
/// device as the blocks it was stored in, which sample to the texels the decoder here produces
/// and take a quarter to an eighth of the space. Without it, or for a texture of another size,
/// they are decoded to RGBA8.
///
/// # Errors
/// Fails with the texture's own error when its format is not one the host decodes or its bytes
/// are shorter than its base level.
pub fn texture_of(
    id: TextureId,
    stored: &PlayerTexture,
    sampler: Option<SamplerState>,
    blocks: bool,
) -> Result<SourceTexture, PlayerTextureError> {
    if stored.format == BC6H {
        return Ok(SourceTexture {
            id,
            size: [stored.width, stored.height],
            graphics_format: BC6H_UFLOAT,
            bytes: block_base_level(stored, "BC6H", 16)?,
            sampler,
        });
    }
    if let Some((graphics_format, block_bytes, name)) = block_upload(stored)
        && blocks
        && stored.width.is_multiple_of(4)
        && stored.height.is_multiple_of(4)
    {
        return Ok(SourceTexture {
            id,
            size: [stored.width, stored.height],
            graphics_format,
            bytes: block_base_level(stored, name, block_bytes)?,
            sampler,
        });
    }
    Ok(SourceTexture {
        id,
        size: [stored.width, stored.height],
        graphics_format: if stored.linear {
            RGBA8_LINEAR
        } else {
            RGBA8_SRGB
        },
        bytes: player_rgba8(stored)?,
        sampler,
    })
}

/// The blocks of a block-compressed texture's base level, rows as the engine stores them. The
/// texture holds 4x4 texel blocks of `block_bytes`; the levels after the base are not carried.
fn block_base_level(
    stored: &PlayerTexture,
    format: &'static str,
    block_bytes: usize,
) -> Result<Vec<u8>, PlayerTextureError> {
    const BLOCK_SIDE: u32 = 4;
    let blocks = |side: u32| side.div_ceil(BLOCK_SIDE) as usize;
    let needed = blocks(stored.width) * blocks(stored.height) * block_bytes;
    stored
        .bytes
        .get(..needed)
        .map(<[u8]>::to_vec)
        .ok_or(PlayerTextureError::Short {
            format,
            width: stored.width,
            height: stored.height,
            needed,
            actual: stored.bytes.len(),
        })
}
