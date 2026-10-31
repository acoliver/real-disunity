//! `Mesh` assets of a player that `MeshFilter` components reference, as plan assets.
//!
//! A mesh plans once per object. The plan entry names its channels, sub-meshes and bounds; the
//! vertex and index data go to `resources/<n>.bin` beside the plan as plain little-endian
//! arrays: one array per planned channel in the order of [`PlanMesh::channels`], each
//! `vertex_count` vertices of `dimension` 32-bit floats, then every index as a 32-bit integer.
//! Position and normal have dimension 3, tangent and color 4, and a texture coordinate 2 to 4;
//! integer and normalized formats are converted to floats, so the managed side reads one format.
//! Vertex data stored in a `.resS` file is read from it. Skin weights and indices are not
//! planned. A mesh the planner cannot decode (mesh compression, a vertex format it does not
//! convert, data that lies outside the object) is left out and listed; a reference to it plans
//! as null.

use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use serde::Serialize;
use worldrunner_unity_serialized::{
    graphics::StreamingInfo,
    mesh::{ChannelInfo, Mesh},
};

use crate::{
    PlayerError,
    player_files::PlayerFiles,
    player_resources::{ObjectKey, Registry, ResourceAsset},
    player_sprites::{locate_stream, write_resource},
};

/// A planned vertex attribute: its name and the number of floats per vertex.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanMeshChannel {
    pub name: String,
    pub dimension: u8,
}

/// A sub-mesh. `first_index` and `index_count` count indices (not bytes) of the mesh's one
/// index array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlanSubMesh {
    pub first_index: u32,
    pub index_count: u32,
    /// The script `MeshTopology` value: 0 triangles, 2 quads, 3 lines, 4 line strip, 5 points.
    pub topology: i32,
    pub base_vertex: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlanBounds {
    pub center: [f32; 3],
    pub extent: [f32; 3],
}

/// What the plan holds of a mesh. `index_format` is the serialized one (0 for 16-bit indices, 1
/// for 32-bit); the resource always holds 32-bit indices. `resource` is the data file's path,
/// relative to the plan's directory.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanMesh {
    pub name: String,
    pub vertex_count: u32,
    pub channels: Vec<PlanMeshChannel>,
    pub sub_meshes: Vec<PlanSubMesh>,
    pub index_count: u32,
    pub index_format: i32,
    pub bounds: PlanBounds,
    pub resource: String,
}

/// A decoded mesh waiting for its asset number: the plan entry and the resource bytes.
pub(crate) struct DecodedMesh {
    pub(crate) plan: PlanMesh,
    bytes: Vec<u8>,
    /// What the plan leaves out of this mesh, listed once.
    pub(crate) notes: Vec<String>,
}

const CHANNEL_NAMES: [&str; 14] = [
    "position",
    "normal",
    "tangent",
    "color",
    "uv0",
    "uv1",
    "uv2",
    "uv3",
    "uv4",
    "uv5",
    "uv6",
    "uv7",
    "blend weights",
    "blend indices",
];
const FIRST_SKIN: usize = 12;
/// Vertex streams start at multiples of this many bytes.
const STREAM_ALIGNMENT: usize = 16;
const MAX_STREAMS: usize = 4;
/// The channel dimension keeps its flags in the high bits.
const DIMENSION_MASK: u8 = 0x0f;

/// How a channel's components are stored, by the serialized format number.
#[derive(Clone, Copy)]
enum Component {
    Float32,
    Float16,
    UNorm8,
    SNorm8,
    UNorm16,
    SNorm16,
    UInt8,
    SInt8,
    UInt16,
    SInt16,
}

impl Component {
    fn from_format(format: u8) -> Option<Self> {
        Some(match format {
            0 => Self::Float32,
            1 => Self::Float16,
            2 => Self::UNorm8,
            3 => Self::SNorm8,
            4 => Self::UNorm16,
            5 => Self::SNorm16,
            6 => Self::UInt8,
            7 => Self::SInt8,
            8 => Self::UInt16,
            9 => Self::SInt16,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Self::UNorm8 | Self::SNorm8 | Self::UInt8 | Self::SInt8 => 1,
            Self::Float16 | Self::UNorm16 | Self::SNorm16 | Self::UInt16 | Self::SInt16 => 2,
            Self::Float32 => 4,
        }
    }

    /// The value of the component at the start of `bytes` as a float.
    fn read(self, bytes: &[u8]) -> f32 {
        let two = || u16::from_le_bytes([bytes[0], bytes[1]]);
        let four = || [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Float32 => f32::from_le_bytes(four()),
            Self::Float16 => half_to_float(two()),
            Self::UNorm8 => f32::from(bytes[0]) / 255.0,
            Self::SNorm8 => (f32::from(bytes[0].cast_signed()) / 127.0).max(-1.0),
            Self::UNorm16 => f32::from(two()) / 65_535.0,
            Self::SNorm16 => (f32::from(two().cast_signed()) / 32_767.0).max(-1.0),
            Self::UInt8 => f32::from(bytes[0]),
            Self::SInt8 => f32::from(bytes[0].cast_signed()),
            Self::UInt16 => f32::from(two()),
            Self::SInt16 => f32::from(two().cast_signed()),
        }
    }
}

/// An IEEE 754 half precision float as a single precision one.
fn half_to_float(half: u16) -> f32 {
    let sign = if half & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((half >> 10) & 0x1f);
    let fraction = f32::from(half & 0x3ff);
    sign * match exponent {
        0 => fraction * 2f32.powi(-24),
        0x1f if fraction == 0.0 => f32::INFINITY,
        0x1f => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}

/// One planned channel of the vertex data: where each vertex's value starts and how it is stored.
struct Source {
    attribute: usize,
    component: Component,
    /// Components in the serialized channel.
    dimension: usize,
    /// Byte offset of the first vertex's value in the vertex data.
    start: usize,
    stride: usize,
}

fn flaw(mesh: &Mesh, why: impl std::fmt::Display) -> String {
    format!("resource mesh {}: {why}", mesh.name)
}

/// The size of one component in a serialized format, including the 32-bit integer formats that
/// only skin channels use.
fn format_size(format: u8) -> Option<usize> {
    match format {
        0 | 10 | 11 => Some(4),
        1 | 4 | 5 | 8 | 9 => Some(2),
        2 | 3 | 6 | 7 => Some(1),
        _ => None,
    }
}

/// The present channels by attribute number.
type Present = Vec<(usize, ChannelInfo)>;

/// The present channels of the vertex data, each with the stream's stride so far.
fn present_channels(mesh: &Mesh) -> Result<(Present, [usize; MAX_STREAMS]), String> {
    let mut present = Vec::new();
    let mut strides = [0usize; MAX_STREAMS];
    for (attribute, channel) in mesh.vertex_data.channels.iter().enumerate() {
        let dimension = usize::from(channel.dimension & DIMENSION_MASK);
        if dimension == 0 {
            continue;
        }
        let name = CHANNEL_NAMES.get(attribute).copied().unwrap_or("unknown");
        let size = format_size(channel.format).ok_or_else(|| {
            let format = channel.format;
            flaw(
                mesh,
                format!("vertex channel {name} has format {format}, which is not decoded"),
            )
        })?;
        let stream = usize::from(channel.stream);
        let stride = strides.get_mut(stream).ok_or_else(|| {
            flaw(
                mesh,
                format!("vertex channel {name} is in stream {stream}, which is not decoded"),
            )
        })?;
        *stride = (*stride).max(usize::from(channel.offset) + dimension * size);
        present.push((attribute, *channel));
    }
    Ok((present, strides))
}

/// The vertex data layout. Streams are laid out one after another, each starting on a 16-byte
/// boundary and holding `vertex_count` records of the stream's stride, which is the end of its
/// furthest channel. The flag says that the mesh has skin channels, which are not planned.
fn sources(mesh: &Mesh) -> Result<(Vec<Source>, bool), String> {
    let vertices = mesh.vertex_data.vertex_count as usize;
    let (present, strides) = present_channels(mesh)?;
    let mut starts = [0usize; MAX_STREAMS];
    let mut end = 0usize;
    for (start, stride) in starts.iter_mut().zip(strides) {
        *start = end.next_multiple_of(STREAM_ALIGNMENT);
        end = *start + stride * vertices;
    }
    let skinned = present
        .iter()
        .any(|&(attribute, _)| attribute >= FIRST_SKIN);
    let mut found = Vec::new();
    for (attribute, channel) in present
        .into_iter()
        .filter(|&(attribute, _)| attribute < FIRST_SKIN)
    {
        let name = CHANNEL_NAMES[attribute];
        let component = Component::from_format(channel.format).ok_or_else(|| {
            let format = channel.format;
            flaw(
                mesh,
                format!("vertex channel {name} has format {format}, which is not decoded"),
            )
        })?;
        let stream = usize::from(channel.stream);
        found.push(Source {
            attribute,
            component,
            dimension: usize::from(channel.dimension & DIMENSION_MASK),
            start: starts[stream] + usize::from(channel.offset),
            stride: strides[stream],
        });
    }
    Ok((found, skinned))
}

/// The number of floats per vertex the plan stores for an attribute.
fn planned_dimension(attribute: usize, serialized: usize) -> u8 {
    let dimension = match attribute {
        0 | 1 => 3,
        2 | 3 => 4,
        _ => serialized.clamp(2, 4),
    };
    u8::try_from(dimension).expect("at most four components")
}

/// Appends the values of `source` for every vertex to `out`, padded or cut to the planned
/// dimension: missing components are 0, or 1 for a color's alpha.
fn append_attribute(source: &Source, data: &[u8], vertices: usize, out: &mut Vec<u8>) {
    let planned = usize::from(planned_dimension(source.attribute, source.dimension));
    let size = source.component.size();
    for vertex in 0..vertices {
        let at = source.start + vertex * source.stride;
        for slot in 0..planned {
            let missing = if source.attribute == 3 { 1.0 } else { 0.0 };
            let value = if slot < source.dimension {
                let from = at + slot * size;
                source.component.read(&data[from..from + size])
            } else {
                missing
            };
            out.extend(value.to_le_bytes());
        }
    }
}

/// The bytes of the vertex data: inline, or read from the `.resS` file the mesh names.
fn vertex_bytes(file: &Path, mesh: &Mesh, object: &[u8]) -> Result<Vec<u8>, PlayerError> {
    if mesh.stream.path.is_empty() {
        return Ok(object
            .get(mesh.vertex_data.data.clone())
            .unwrap_or_default()
            .to_vec());
    }
    read_stream(file, &mesh.stream, &format!("mesh {}", mesh.name))
}

fn read_stream(file: &Path, stream: &StreamingInfo, what: &str) -> Result<Vec<u8>, PlayerError> {
    let located = locate_stream(file, stream, what)?;
    let io = |source| PlayerError::Io {
        path: located.file.clone(),
        source,
    };
    let mut open = std::fs::File::open(&located.file).map_err(io)?;
    open.seek(SeekFrom::Start(located.offset)).map_err(io)?;
    let mut bytes = vec![0; usize::try_from(located.size).expect("stream sizes are 32-bit")];
    open.read_exact(&mut bytes).map_err(io)?;
    Ok(bytes)
}

/// The mesh's indices as 32-bit values and its sub-meshes, or why they are not usable.
fn indices(mesh: &Mesh, object: &[u8]) -> Result<(Vec<u32>, Vec<PlanSubMesh>), String> {
    let raw = object
        .get(mesh.index_buffer.clone())
        .ok_or_else(|| flaw(mesh, "its index data lies outside the object"))?;
    let width = if mesh.index_format == 0 { 2 } else { 4 };
    if !raw.len().is_multiple_of(width) {
        return Err(flaw(
            mesh,
            "its index data is not a whole number of indices",
        ));
    }
    let list: Vec<u32> = raw
        .chunks_exact(width)
        .map(|c| {
            if width == 2 {
                u32::from(u16::from_le_bytes([c[0], c[1]]))
            } else {
                u32::from_le_bytes([c[0], c[1], c[2], c[3]])
            }
        })
        .collect();
    let mut subs = Vec::with_capacity(mesh.sub_meshes.len());
    for (number, sub) in mesh.sub_meshes.iter().enumerate() {
        if !matches!(sub.topology, 0 | 2 | 3 | 4 | 5) {
            return Err(flaw(
                mesh,
                format!(
                    "sub-mesh {number} has topology {}, which is not built",
                    sub.topology
                ),
            ));
        }
        let first = sub.first_byte as usize / width;
        if !(sub.first_byte as usize).is_multiple_of(width)
            || first + sub.index_count as usize > list.len()
        {
            return Err(flaw(
                mesh,
                format!("sub-mesh {number} lies outside the index data"),
            ));
        }
        subs.push(PlanSubMesh {
            first_index: u32::try_from(first).expect("index counts are 32-bit"),
            index_count: sub.index_count,
            topology: sub.topology,
            base_vertex: sub.base_vertex,
            first_vertex: sub.first_vertex,
            vertex_count: sub.vertex_count,
        });
    }
    Ok((list, subs))
}

/// Decodes the mesh `key` into its plan entry and resource bytes. The inner error says why the
/// mesh is left out.
pub(crate) fn decode_mesh(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<Result<DecodedMesh, String>, PlayerError> {
    let (file, path_id) = key;
    let open = files.open_path(file)?;
    let mesh: Mesh = open.decode(*path_id)?;
    let object = *open.file.object(*path_id).expect("decoded objects exist");
    let object_bytes = open.bytes(&object)?;
    if mesh.compression != 0 {
        return Ok(Err(flaw(&mesh, "compressed meshes are not decoded yet")));
    }
    let data = vertex_bytes(file, &mesh, &object_bytes)?;
    Ok(build(&mesh, &data, &object_bytes))
}

fn build(mesh: &Mesh, data: &[u8], object: &[u8]) -> Result<DecodedMesh, String> {
    let vertices = mesh.vertex_data.vertex_count as usize;
    let (layout, skinned) = sources(mesh)?;
    let short = |s: &Source| {
        s.start + s.stride * (vertices - 1) + s.dimension * s.component.size() > data.len()
    };
    if vertices > 0 && layout.iter().any(short) {
        return Err(flaw(
            mesh,
            "its vertex data is shorter than its channels need",
        ));
    }
    let mut bytes = Vec::new();
    for source in &layout {
        append_attribute(source, data, vertices, &mut bytes);
    }
    let channels = layout
        .iter()
        .map(|source| PlanMeshChannel {
            name: CHANNEL_NAMES[source.attribute].to_owned(),
            dimension: planned_dimension(source.attribute, source.dimension),
        })
        .collect();
    let (list, sub_meshes) = indices(mesh, object)?;
    bytes.extend(list.iter().flat_map(|index| index.to_le_bytes()));
    let aabb = mesh.local_aabb;
    Ok(DecodedMesh {
        plan: PlanMesh {
            name: mesh.name.clone(),
            vertex_count: mesh.vertex_data.vertex_count,
            channels,
            sub_meshes,
            index_count: u32::try_from(list.len()).expect("index counts are 32-bit"),
            index_format: mesh.index_format,
            bounds: PlanBounds {
                center: aabb.center,
                extent: aabb.extent,
            },
            resource: String::new(),
        },
        bytes,
        notes: if skinned {
            vec![flaw(mesh, "skin weights and indices are not planned")]
        } else {
            Vec::new()
        },
    })
}

/// Writes the resource of the decoded mesh, asset number `index`, and returns its plan asset.
pub(crate) fn plan_mesh(
    registry: &mut Registry,
    key: &ObjectKey,
    output: &Path,
    index: usize,
) -> Result<ResourceAsset, PlayerError> {
    let mut decoded = registry
        .take_mesh(key)
        .expect("a mesh is decoded when it is admitted");
    let relative = format!("resources/{index}.bin");
    write_resource(output, &relative, &decoded.bytes)?;
    decoded.plan.resource = relative;
    Ok(ResourceAsset::Mesh { mesh: decoded.plan })
}
