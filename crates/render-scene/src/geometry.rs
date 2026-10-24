//! Packing governed geometry into GPU buffers.
//!
//! The scene domain stores coordinates as exact integers — millimetres for
//! positions, millionths for directions, texture coordinates and scale — so
//! that composition is reproducible and comparable. GPUs consume `f32`. This
//! module performs that conversion in one place, and nowhere else.
//!
//! # Why `f32` metres is enough
//!
//! An `f32` carries a 24-bit mantissa, so its resolution at magnitude `m` is
//! about `m * 2^-23`. Working in metres, a vertex ten kilometres from the
//! origin resolves to roughly 1.2 mm, and one at a hundred kilometres to
//! 12 mm. The authored world is far smaller than the first of those bounds, so
//! plain world-space `f32` is exact enough that no camera-relative rebasing is
//! needed. Recording the bound matters more than the conclusion: if the world
//! ever grows past it, the symptom is vertices visibly crawling as the camera
//! moves, and this comment is the place that predicts it.
//!
//! # Why there is no normal matrix
//!
//! Transforming a normal correctly generally needs the inverse transpose of
//! the model matrix, because non-uniform scale shears normals away from their
//! surface. [`SceneTransform`] carries a single `scale_millionths` applied to
//! all three axes, and the inverse transpose of a rotation composed with a
//! uniform scale is that same rotation scaled by a positive constant — which
//! renormalising discards. So the model matrix transforms normals correctly on
//! its own, and shipping a second matrix per instance would cost bandwidth to
//! deliver a value already present.
//!
//! # Why billboard mode travels per instance
//!
//! A billboard's orientation depends on where the camera is, so it cannot be
//! baked into the model matrix at upload time without repacking every instance
//! buffer each frame — which would dissolve the instanced batching the draw
//! loop is built on. The mode therefore rides along as a per-instance integer
//! and the vertex stage derives the facing from the camera position it already
//! holds. Every instance in a batch shares a mesh LOD and so shares this value;
//! the four redundant bytes buy the ability to leave the pipeline layout, the
//! bind groups and the per-draw state entirely alone.

use glam::{Mat4, Quat, Vec3};
use thiserror::Error;
use worldrunner_render_plan::{BillboardPolicy, SceneMeshLod, SceneTransform, SceneVertex};

/// Millimetres in one metre.
const MILLIMETERS_PER_METER: f64 = 1_000.0;

/// Micro-units in one whole unit.
const MICRO_SCALE: f64 = 1_000_000.0;

/// Bytes occupied by one packed vertex, as a buffer length.
pub(crate) const VERTEX_BYTES: usize = 32;

/// Bytes occupied by one packed instance, as a buffer length.
const INSTANCE_BYTES: usize = 68;

/// Bytes occupied by one packed vertex: position, normal, texture coordinate.
pub const VERTEX_STRIDE: u64 = VERTEX_BYTES as u64;

/// Bytes occupied by one packed instance: a column-major `mat4`, then a mode.
pub const INSTANCE_STRIDE: u64 = INSTANCE_BYTES as u64;

/// Billboard mode for geometry drawn exactly as authored.
///
/// [`BillboardPolicy::CrossQuads`] shares this code rather than taking one of
/// its own: crossed quads earn their approximation precisely by *not* turning,
/// so the renderer has nothing to do for them beyond drawing the authored
/// geometry. Distinguishing the two here would imply a difference the vertex
/// stage does not have.
pub const BILLBOARD_FIXED: u32 = 0;

/// Billboard mode for a card that pivots about the scene's vertical axis.
pub const BILLBOARD_YAW_LOCKED: u32 = 1;

/// Billboard mode for a card that faces the camera on every axis.
pub const BILLBOARD_SPHERICAL: u32 = 2;

/// Governed geometry could not be packed for the GPU.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum GeometryError {
    /// A vertex normal had no direction.
    #[error("vertex {index} has a zero-length normal, which has no direction to shade with")]
    DegenerateNormal {
        /// Index of the offending vertex within its LOD.
        index: usize,
    },
}

/// Packs one mesh LOD's vertices into a GPU vertex buffer.
///
/// # Errors
///
/// Returns [`GeometryError::DegenerateNormal`] when a vertex normal is zero.
/// Normalising such a normal yields `NaN`, and `NaN` spreads through every
/// lighting term that touches it, so the visible result is a black or garbage
/// surface far from the data that caused it. Substituting an arbitrary
/// direction would hide the authoring fault behind plausible shading instead.
pub fn pack_vertices(lod: &SceneMeshLod) -> Result<Vec<u8>, GeometryError> {
    let vertices = lod.vertices();
    let mut bytes = Vec::with_capacity(vertices.len() * VERTEX_BYTES);
    for (index, vertex) in vertices.iter().enumerate() {
        pack_vertex(vertex, index, &mut bytes)?;
    }
    Ok(bytes)
}

/// Appends one packed vertex to a buffer.
fn pack_vertex(
    vertex: &SceneVertex,
    index: usize,
    bytes: &mut Vec<u8>,
) -> Result<(), GeometryError> {
    let position = millimeters_to_meters(vertex.position_millimeters);
    let normal = Vec3::new(
        micro_to_unit(vertex.normal_micro[0]),
        micro_to_unit(vertex.normal_micro[1]),
        micro_to_unit(vertex.normal_micro[2]),
    );
    let normal = normal
        .try_normalize()
        .ok_or(GeometryError::DegenerateNormal { index })?;

    push_vec3(bytes, position);
    push_vec3(bytes, normal);
    // Texture coordinates are deliberately not clamped into the unit square:
    // tiled surfaces address themselves with coordinates well beyond it, and
    // clamping would collapse the repeat into a single stretched texel.
    push_f32(bytes, micro_to_unit(vertex.texcoord_micro[0]));
    push_f32(bytes, micro_to_unit(vertex.texcoord_micro[1]));
    Ok(())
}

/// Packs one mesh LOD's indices into a GPU index buffer.
#[must_use]
pub fn pack_indices(lod: &SceneMeshLod) -> Vec<u8> {
    let indices = lod.indices();
    let mut bytes = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes
}

/// Builds the model matrix that places a mesh in the scene.
///
/// The matrix stays in the scene's native Z-up frame. The single change to a
/// Y-up basis happens at the view level; applying it here as well would apply
/// it twice, and applying it here *instead* would leave mesh vertices in one
/// frame and their placements in another — the standard way to ship a world
/// that is mirrored in a manner nobody can localise.
#[must_use]
pub fn model_matrix(transform: &SceneTransform) -> Mat4 {
    let rotation = Quat::from_xyzw(
        micro_to_unit(transform.rotation.x),
        micro_to_unit(transform.rotation.y),
        micro_to_unit(transform.rotation.z),
        micro_to_unit(transform.rotation.w),
    )
    // The domain admits rotations up to a proven quantisation tolerance rather
    // than exactly unit ones, so this renormalises what validation permitted.
    .normalize();
    let scale = scale_to_unit(transform.scale_millionths);
    Mat4::from_scale_rotation_translation(
        Vec3::splat(scale),
        rotation,
        millimeters_to_meters(transform.position_millimeters),
    )
}

/// Packs a model matrix and billboard mode into instance bytes.
///
/// The matrix is column-major; the mode follows it as a single `u32`.
#[must_use]
pub fn pack_instance(transform: &SceneTransform, billboard: Option<BillboardPolicy>) -> Vec<u8> {
    pack_model_instance(model_matrix(transform), billboard_mode(billboard))
}

pub(crate) fn pack_model_instance(matrix: Mat4, mode: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(INSTANCE_BYTES);
    for column in matrix.to_cols_array() {
        push_f32(&mut bytes, column);
    }
    bytes.extend_from_slice(&mode.to_le_bytes());
    bytes
}

/// Maps a governed billboard policy to the code the vertex stage branches on.
///
/// The mapping is exhaustive on purpose: adding a policy to the domain should
/// fail to compile here rather than silently fall through to a default, which
/// would render the new kind as ordinary geometry and look merely wrong.
#[must_use]
pub const fn billboard_mode(billboard: Option<BillboardPolicy>) -> u32 {
    match billboard {
        None | Some(BillboardPolicy::CrossQuads) => BILLBOARD_FIXED,
        Some(BillboardPolicy::YawLocked) => BILLBOARD_YAW_LOCKED,
        Some(BillboardPolicy::Spherical) => BILLBOARD_SPHERICAL,
    }
}

/// Describes the packed vertex layout to `wgpu`.
#[must_use]
pub const fn vertex_attributes() -> [wgpu::VertexAttribute; 3] {
    [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 12,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 24,
            shader_location: 2,
        },
    ]
}

/// Describes the packed instance layout to `wgpu`.
#[must_use]
pub const fn instance_attributes() -> [wgpu::VertexAttribute; 5] {
    [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 32,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 48,
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 64,
            shader_location: 7,
        },
    ]
}

/// Appends a vector as three little-endian `f32`s.
pub(crate) fn push_vec3(bytes: &mut Vec<u8>, value: Vec3) {
    push_f32(bytes, value.x);
    push_f32(bytes, value.y);
    push_f32(bytes, value.z);
}

/// Appends one little-endian `f32`.
///
/// Written out by hand because the workspace forbids `unsafe`, so a packed
/// buffer cannot be reinterpreted from a struct.
pub(crate) fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

/// Converts a millimetre triple to metres.
pub(crate) fn millimeters_to_meters(value: [i64; 3]) -> Vec3 {
    Vec3::new(
        millimeter_to_meter(value[0]),
        millimeter_to_meter(value[1]),
        millimeter_to_meter(value[2]),
    )
}

/// Converts whole millimetres to metres.
#[expect(
    clippy::cast_precision_loss,
    reason = "the f64 intermediate is exact for every magnitude the authored world reaches"
)]
pub(crate) fn millimeter_to_meter(value: i64) -> f32 {
    narrow(value as f64 / MILLIMETERS_PER_METER)
}

/// Converts micro-units to whole units.
#[expect(
    clippy::cast_precision_loss,
    reason = "micro-unit ratios are far below the magnitude where f64 loses integers"
)]
fn micro_to_unit(value: i64) -> f32 {
    narrow(value as f64 / MICRO_SCALE)
}

/// Converts millionths of scale to a multiplier.
#[expect(
    clippy::cast_precision_loss,
    reason = "authored scales are far below the magnitude where f64 loses integers"
)]
fn scale_to_unit(value: u64) -> f32 {
    narrow(value as f64 / MICRO_SCALE)
}

/// Narrows to the width the GPU consumes.
#[expect(
    clippy::cast_possible_truncation,
    reason = "narrowing to f32 is the entire purpose of this conversion"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests;
