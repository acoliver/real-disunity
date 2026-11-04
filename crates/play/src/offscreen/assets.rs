//! The meshes and textures the managed engine stored, as render-program's meshes and source
//! textures.
//!
//! Vertex channels are numbered as the shaders' vertex input tables number them. The numbers 0
//! to 4 follow the channel order of `unity-import`'s mesh planner: position, normal, tangent,
//! color, then the first texture coordinates. render-program's own tests draw with 0 for
//! position and 4 for texture coordinates, and the counts in its README (0, 1 and 4 read by
//! most variants, 3 by a third) fit that order. A stored mesh holds no tangents, so a shader
//! that reads channel 2 gets `MissingChannel`.

use worldrunner_disunity_runtime::{DisunityHost, MeshRun, PlayerMesh, PlayerTexture};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, MeshIndices, SourceTexture, SubMesh,
};
use worldrunner_world_simulation::MAX_RENDERER_MATERIALS;

use crate::textures::{PlayerTextureError, player_rgba8};

pub const POSITION: i8 = 0;
pub const NORMAL: i8 = 1;
pub const COLOR: i8 = 3;
pub const TEXTURE_COORDINATES: i8 = 4;

/// `GraphicsFormat` codes of 8-bit RGBA, in linear and in sRGB.
const RGBA8_LINEAR: u32 = 8;
const RGBA8_SRGB: u32 = 4;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AssetError(pub String);

/// Where the host reads what the managed engine stored.
pub trait Assets {
    /// The mesh stored under `identity`, or `None` when none is stored or its generation is
    /// still `known_generation`.
    fn mesh(&self, identity: u64, known_generation: u64) -> Result<Option<PlayerMesh>, AssetError>;

    /// The texture stored under `identity`, or `None` when none is stored or its generation is
    /// still `known_generation`.
    fn texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, AssetError>;
}

impl Assets for DisunityHost {
    fn mesh(&self, identity: u64, known_generation: u64) -> Result<Option<PlayerMesh>, AssetError> {
        self.player_mesh(identity, known_generation)
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
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeshError {
    #[error("the mesh has no triangles")]
    NoTriangles,
    #[error("the mesh has {found} {what} for {expected} positions")]
    ChannelLength {
        what: &'static str,
        found: usize,
        expected: usize,
    },
    #[error("the mesh has {0} positions, more than a vertex count holds")]
    TooManyVertices(usize),
    #[error("a sub-mesh is numbered {0}, past the {MAX_SLOTS} a renderer holds")]
    SubMeshSlot(u32),
}

/// Most material slots, and so most sub-meshes, the world gives a renderer.
const MAX_SLOTS: u32 = MAX_RENDERER_MATERIALS;

fn floats<const N: usize>(values: &[[f32; N]]) -> Vec<u8> {
    values
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// The sub-meshes by the slot number the managed mesh gave them. A sub-mesh the engine left out
/// leaves a gap, which draws nothing.
fn sub_meshes(runs: &[MeshRun]) -> Result<Vec<SubMesh>, MeshError> {
    if let Some(run) = runs.iter().find(|run| run.slot >= MAX_SLOTS) {
        return Err(MeshError::SubMeshSlot(run.slot));
    }
    let count = runs
        .iter()
        .map(|run| run.slot as usize + 1)
        .max()
        .unwrap_or(0);
    let mut subs = vec![
        SubMesh {
            first: 0,
            count: 0,
            base_vertex: 0,
        };
        count
    ];
    for run in runs {
        subs[run.slot as usize] = SubMesh {
            first: run.first_index,
            count: run.index_count,
            base_vertex: 0,
        };
    }
    Ok(subs)
}

/// A stored mesh as a render-program mesh.
///
/// # Errors
/// Fails when the mesh has no triangles, a sub-mesh has a number no renderer has a slot for,
/// or an attribute that is present does not have one entry
/// for each position.
pub fn mesh_of(stored: &PlayerMesh) -> Result<Mesh, MeshError> {
    let vertices = stored.positions.len();
    let vertex_count = u32::try_from(vertices).map_err(|_| MeshError::TooManyVertices(vertices))?;
    if vertices == 0 || stored.indices.is_empty() {
        return Err(MeshError::NoTriangles);
    }
    let mut channels = vec![MeshChannel {
        source: POSITION,
        format: ChannelFormat::Float32x3,
        bytes: floats(&stored.positions),
    }];
    let mut optional = |what, source, format, found: usize, bytes: Vec<u8>| {
        if found == 0 {
            return Ok(());
        }
        if found != vertices {
            return Err(MeshError::ChannelLength {
                what,
                found,
                expected: vertices,
            });
        }
        channels.push(MeshChannel {
            source,
            format,
            bytes,
        });
        Ok(())
    };
    optional(
        "normals",
        NORMAL,
        ChannelFormat::Float32x3,
        stored.normals.len(),
        floats(&stored.normals),
    )?;
    optional(
        "colors",
        COLOR,
        ChannelFormat::Float32x4,
        stored.colors.len(),
        floats(&stored.colors),
    )?;
    optional(
        "texture coordinates",
        TEXTURE_COORDINATES,
        ChannelFormat::Float32x2,
        stored.uvs.len(),
        floats(&stored.uvs),
    )?;
    Ok(Mesh {
        vertex_count,
        channels,
        indices: MeshIndices::U32(stored.indices.clone()),
        sub_meshes: sub_meshes(&stored.submeshes)?,
    })
}

/// A stored texture's base level as a source texture. Its rows stay bottom first, as the engine
/// stores them, which is the orientation of a render texture's rows (see
/// `camera::clip_from_gl`). Its sampler is the fixed one: the stored texture does not say how
/// it wraps or filters.
///
/// # Errors
/// Fails with the texture's own error when its format is not one the host decodes or its bytes
/// are shorter than its base level.
pub fn texture_of(
    id: TextureId,
    stored: &PlayerTexture,
) -> Result<SourceTexture, PlayerTextureError> {
    Ok(SourceTexture {
        id,
        size: [stored.width, stored.height],
        graphics_format: if stored.linear {
            RGBA8_LINEAR
        } else {
            RGBA8_SRGB
        },
        bytes: player_rgba8(stored)?,
        sampler: None,
    })
}
