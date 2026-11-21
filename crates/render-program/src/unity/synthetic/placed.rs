//! A blit pass whose vertex stage places the blit quad by the built-in matrices
//! `unity_ObjectToWorld` and `unity_MatrixVP`, as a player's blit passes may. With no value for
//! either, the matrices pack as zero and the quad collapses to a point.

use super::shaders::{no_cull, variant};
use super::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, binding, constant_buffer, matrix_member, shader,
    slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};

/// Places each vertex by the two matrices and returns the source times the material's `_Tint`.
pub const PLACED_BLIT: &str = "
struct Material { tint: vec4<f32> }
struct Draw { object_to_world: mat4x4<f32>, view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(2) var source: texture_2d<f32>;
@group(0) @binding(3) var source_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = draw.view_projection * (draw.object_to_world * vec4<f32>(position, 1.0));
    out.uv = uv;
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, in.uv) * material.tint;
}";

/// The constants of [`PLACED_BLIT`]: the fragment stage's `_Tint`, the vertex stage's
/// `unity_ObjectToWorld` and `unity_MatrixVP`, and the source texture under the name `main`.
pub fn placed_blit_parameters(main: &str) -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![
            constant_buffer("Material", 16, vec![vector_member("_Tint", 0)]),
            constant_buffer(
                "Draw",
                128,
                vec![
                    matrix_member("unity_ObjectToWorld", 0),
                    matrix_member("unity_MatrixVP", 64),
                ],
            ),
        ],
        bindings: vec![
            binding(
                "Material",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
            binding(
                main,
                slot(0, 2, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 3, FRAGMENT)),
                    dimension: 4,
                },
            ),
            binding("", slot(0, 3, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    }
}

/// One blit pass that draws with [`PLACED_BLIT`]; its source is the texture property `main`.
pub fn placed_blit(main: &str) -> SyntheticShader {
    no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                PLACED_BLIT,
                "fs_main",
                placed_blit_parameters(main),
            )],
        }],
    ))
}
