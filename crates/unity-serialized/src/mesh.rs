//! The `Mesh` layout: sub-meshes, skinning and blend shape data, index and vertex buffers.
//!
//! Index and vertex bytes are returned as ranges within the object; when a mesh streams its
//! vertex data from a `.resS` file, [`Mesh::stream`] says where.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    graphics::StreamingInfo,
    object::{Layout, ObjectReader},
};

/// A box as center and extent (half size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub center: [f32; 3],
    pub extent: [f32; 3],
}

impl Aabb {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            center: r.floats()?,
            extent: r.floats()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubMesh {
    /// Offset of the first index, in bytes into the index buffer.
    pub first_byte: u32,
    pub index_count: u32,
    /// Unity's `MeshTopology`: 0 triangles, 2 quads, 3 lines, 4 line strip, 5 points.
    pub topology: i32,
    pub base_vertex: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub local_aabb: Aabb,
}

const SUB_MESH_LEN: usize = 48;

impl SubMesh {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            first_byte: r.u32()?,
            index_count: r.u32()?,
            topology: r.i32()?,
            base_vertex: r.u32()?,
            first_vertex: r.u32()?,
            vertex_count: r.u32()?,
            local_aabb: Aabb::read(r)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendShapeVertex {
    pub vertex: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendShape {
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub has_normals: bool,
    pub has_tangents: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlendShapeChannel {
    pub name: String,
    pub name_hash: u32,
    pub frame_index: i32,
    pub frame_count: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlendShapeData {
    pub vertices: Vec<BlendShapeVertex>,
    pub shapes: Vec<BlendShape>,
    pub channels: Vec<BlendShapeChannel>,
    pub full_weights: Vec<f32>,
}

impl BlendShapeData {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            vertices: r.array("blend shape vertex", 40, |r| {
                Ok(BlendShapeVertex {
                    vertex: r.floats()?,
                    normal: r.floats()?,
                    tangent: r.floats()?,
                    index: r.u32()?,
                })
            })?,
            shapes: r.array("blend shape", 12, |r| {
                let shape = BlendShape {
                    first_vertex: r.u32()?,
                    vertex_count: r.u32()?,
                    has_normals: r.bool()?,
                    has_tangents: r.bool()?,
                };
                r.align()?;
                Ok(shape)
            })?,
            channels: r.array("blend shape channel", 16, |r| {
                Ok(BlendShapeChannel {
                    name: r.string()?,
                    name_hash: r.u32()?,
                    frame_index: r.i32()?,
                    frame_count: r.i32()?,
                })
            })?,
            full_weights: r.array("blend shape weight", 4, ObjectReader::f32)?,
        })
    }
}

/// Bone data of a skinned mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skin {
    /// Bind poses as row-major 4x4 matrices (`e00`, `e01`, ...).
    pub bind_poses: Vec<[f32; 16]>,
    pub bone_name_hashes: Vec<u32>,
    pub root_bone_name_hash: u32,
    /// Bone bounds as minimum and maximum corners.
    pub bones_aabb: Vec<[[f32; 3]; 2]>,
    pub variable_bone_count_weights: Vec<u32>,
}

impl Skin {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            bind_poses: r.array("bind pose", 64, ObjectReader::floats)?,
            bone_name_hashes: r.array("bone name hash", 4, ObjectReader::u32)?,
            root_bone_name_hash: r.u32()?,
            bones_aabb: r.array("bone bounds", 24, |r| Ok([r.floats()?, r.floats()?]))?,
            variable_bone_count_weights: r.array("bone weight", 4, ObjectReader::u32)?,
        })
    }
}

/// How one vertex attribute is stored in the vertex data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelInfo {
    pub stream: u8,
    pub offset: u8,
    /// Unity's `VertexAttributeFormat`, for example 0 for 32-bit float.
    pub format: u8,
    /// Component count. Unity may set high bits as flags.
    pub dimension: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexData {
    pub vertex_count: u32,
    /// Attribute channels in Unity's fixed order: position, normal, tangent, color, then
    /// texture coordinates and blend weights and indices.
    pub channels: Vec<ChannelInfo>,
    /// Byte range of the vertex data within the object; empty when the data is streamed.
    pub data: Range<usize>,
}

impl VertexData {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let data = Self {
            vertex_count: r.u32()?,
            channels: r.array("vertex channel", 4, |r| {
                Ok(ChannelInfo {
                    stream: r.u8()?,
                    offset: r.u8()?,
                    format: r.u8()?,
                    dimension: r.u8()?,
                })
            })?,
            data: r.byte_range("vertex data byte")?,
        };
        r.align()?;
        Ok(data)
    }
}

/// Values quantized to `bit_size` bits each. `range` and `start` map them back to floats;
/// they are absent for integer vectors.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedBitVector {
    pub num_items: u32,
    /// Range and start of the float encoding.
    pub float_range: Option<[f32; 2]>,
    /// Byte range of the packed bits within the object.
    pub data: Range<usize>,
    pub bit_size: u8,
}

impl PackedBitVector {
    fn read(r: &mut ObjectReader<'_>, floats: bool) -> Result<Self, SerializedError> {
        let num_items = r.u32()?;
        let float_range = if floats { Some(r.floats()?) } else { None };
        let data = r.byte_range("packed bit")?;
        let bit_size = r.u8()?;
        r.align()?;
        Ok(Self {
            num_items,
            float_range,
            data,
            bit_size,
        })
    }
}

/// A mesh stored with Unity's mesh compression. Uncompressed meshes leave every vector empty.
#[derive(Clone, Debug, PartialEq)]
pub struct CompressedMesh {
    pub vertices: PackedBitVector,
    pub uv: PackedBitVector,
    pub normals: PackedBitVector,
    pub tangents: PackedBitVector,
    pub weights: PackedBitVector,
    pub normal_signs: PackedBitVector,
    pub tangent_signs: PackedBitVector,
    pub float_colors: PackedBitVector,
    pub bone_indices: PackedBitVector,
    pub triangles: PackedBitVector,
    pub uv_info: u32,
}

impl CompressedMesh {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            vertices: PackedBitVector::read(r, true)?,
            uv: PackedBitVector::read(r, true)?,
            normals: PackedBitVector::read(r, true)?,
            tangents: PackedBitVector::read(r, true)?,
            weights: PackedBitVector::read(r, false)?,
            normal_signs: PackedBitVector::read(r, false)?,
            tangent_signs: PackedBitVector::read(r, false)?,
            float_colors: PackedBitVector::read(r, true)?,
            bone_indices: PackedBitVector::read(r, false)?,
            triangles: PackedBitVector::read(r, false)?,
            uv_info: r.u32()?,
        })
    }
}

/// Level-of-detail ranges of a mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshLodInfo {
    pub lod_slope: f32,
    pub lod_bias: f32,
    pub num_levels: i32,
    /// Per sub-mesh, each level's first index and index count.
    pub sub_meshes: Vec<Vec<[u32; 2]>>,
}

impl MeshLodInfo {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            lod_slope: r.f32()?,
            lod_bias: r.f32()?,
            num_levels: r.i32()?,
            sub_meshes: r.array("lod sub-mesh", 4, |r| {
                r.array("lod level", 8, |r| Ok([r.u32()?, r.u32()?]))
            })?,
        })
    }
}

/// Collision data Unity baked for the mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedCollision {
    pub cooking_options: i32,
    /// Byte ranges of the baked convex and triangle meshes within the object.
    pub convex: Range<usize>,
    pub triangles: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub name: String,
    pub sub_meshes: Vec<SubMesh>,
    pub blend_shapes: BlendShapeData,
    pub skin: Skin,
    /// Unity's `ModelImporterMeshCompression`: 0 is uncompressed.
    pub compression: u8,
    pub is_readable: bool,
    pub keep_vertices: bool,
    pub keep_indices: bool,
    /// 0 for 16-bit indices, 1 for 32-bit.
    pub index_format: i32,
    /// Byte range of the index buffer within the object.
    pub index_buffer: Range<usize>,
    pub vertex_data: VertexData,
    pub compressed: CompressedMesh,
    pub local_aabb: Aabb,
    pub usage_flags: i32,
    pub collision: BakedCollision,
    pub metrics: [f32; 2],
    pub stream: StreamingInfo,
    pub lod: MeshLodInfo,
}

impl Layout for Mesh {
    const CLASS: ClassId = ClassId::MESH;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let sub_meshes = r.array("sub-mesh", SUB_MESH_LEN, SubMesh::read)?;
        let blend_shapes = BlendShapeData::read(r)?;
        let skin = Skin::read(r)?;
        let compression = r.u8()?;
        let [is_readable, keep_vertices, keep_indices] = [r.bool()?, r.bool()?, r.bool()?];
        r.align()?;
        Ok(Self {
            name,
            sub_meshes,
            blend_shapes,
            skin,
            compression,
            is_readable,
            keep_vertices,
            keep_indices,
            index_format: r.i32()?,
            index_buffer: r.byte_range("index byte")?,
            vertex_data: VertexData::read(r)?,
            compressed: CompressedMesh::read(r)?,
            local_aabb: Aabb::read(r)?,
            usage_flags: r.i32()?,
            collision: BakedCollision {
                cooking_options: r.i32()?,
                convex: r.byte_range("baked convex byte")?,
                triangles: r.byte_range("baked triangle byte")?,
            },
            metrics: r.floats()?,
            stream: StreamingInfo::read(r)?,
            lod: MeshLodInfo::read(r)?,
        })
    }
}
