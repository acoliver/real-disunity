//! Tiny player-shaped shaders: WGSL stand-ins with the records, parameters and vertex input
//! tables a Unity player stores for them.

use worldrunner_render_program::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, VariantSpec, binding, constant_buffer, shader,
    slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};

/// A vertex stage that places its position by the draw constants `_Place` (scale x, scale y,
/// offset x, offset y), and fragment stages that return the material's `_Tint`, or the tint with
/// blue raised by a quarter.
pub const TINT: &str = "
struct Material { tint: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> { return material.tint; }
@fragment fn fs_bluer(in: VOut) -> @location(0) vec4<f32> {
    return material.tint + vec4<f32>(0.0, 0.0, 0.25, 0.0);
}";

/// Position (source channel 0) feeds input 0 and the first texture coordinate (source channel
/// 4) feeds input 1.
pub const POSITION_AND_UV: [[i8; 2]; 2] = [[0, 0], [4, 1]];

/// The `_Tint` constants of the fragment stage at binding 0 and the `_Place` constants of the
/// vertex stage at binding 1.
pub fn tint_parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![
            constant_buffer("Material", 16, vec![vector_member("_Tint", 0)]),
            constant_buffer("Draw", 16, vec![vector_member("_Place", 0)]),
        ],
        bindings: vec![
            binding(
                "Material",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
        ],
    }
}

pub fn tint_variant(keywords: &[&'static str], fragment: &'static str) -> VariantSpec {
    VariantSpec {
        keywords: keywords.to_vec(),
        wgsl: TINT.to_owned(),
        entries: ["vs_main", fragment],
        parameters: tint_parameters(),
        channels: POSITION_AND_UV.to_vec(),
    }
}

/// Two passes tagged `LightMode` `Alpha` and `Beta`. Keywords `K_BLUER` and `K_OTHER` are the
/// shader's. `Alpha` has the variants `{}` and `{K_BLUER}` and `Beta` has `{K_OTHER}` and
/// `{K_BLUER, K_OTHER}`, so `Beta` has no variant for the empty set.
pub fn two_passes() -> SyntheticShader {
    shader(
        &["K_BLUER", "K_OTHER"],
        &[
            PassSpec {
                tags: vec![("LIGHTMODE", "Alpha")],
                variants: vec![
                    tint_variant(&[], "fs_main"),
                    tint_variant(&["K_BLUER"], "fs_bluer"),
                ],
            },
            PassSpec {
                tags: vec![("LIGHTMODE", "Beta")],
                variants: vec![
                    tint_variant(&["K_OTHER"], "fs_main"),
                    tint_variant(&["K_BLUER", "K_OTHER"], "fs_bluer"),
                ],
            },
        ],
    )
}
