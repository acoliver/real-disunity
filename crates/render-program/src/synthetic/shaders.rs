//! Tiny player-shaped shaders for tests: WGSL stand-ins with the records, parameters and vertex
//! input tables a Unity player stores for them, and the meshes they draw.

use super::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, VariantSpec, binding, constant_buffer, fixed,
    shader, slot, vector_member,
};
use crate::offscreen::frame::{ChannelFormat, Mesh, MeshChannel, MeshIndices, SubMesh};
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

/// A textured camera pass: the albedo texture sampled half a texture to the right of the
/// fragment, times the material's tint. `place.z` is the depth of the quad.
pub const ALBEDO: &str = "
struct Material { tint: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(2) var albedo: texture_2d<f32>;
@group(0) @binding(3) var albedo_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_shifted(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(albedo, albedo_sampler, in.uv + vec2<f32>(0.5, 0.0)) * material.tint;
}";

/// A blit pass: the source texture times the tint, and a variant that shows the texel size.
pub const BLIT: &str = "
struct Material { tint: vec4<f32>, texel: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var main_tex: texture_2d<f32>;
@group(0) @binding(2) var main_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, in.uv) * material.tint;
}
@fragment fn fs_texel(in: VOut) -> @location(0) vec4<f32> {
    // The texture and tint are read so that this pass binds what the others do; they add zero.
    let unused = textureSample(main_tex, main_sampler, in.uv) * material.tint * 0.0;
    return unused + vec4<f32>(
        f32(material.texel.z == 4.0),
        f32(material.texel.w == 2.0),
        f32(abs(material.texel.x - 0.25) < 0.000001),
        1.0,
    );
}
@fragment fn fs_inverted(in: VOut) -> @location(0) vec4<f32> {
    let texel = textureSample(main_tex, main_sampler, in.uv);
    let unused = material.tint * 0.0 + material.texel * 0.0;
    return unused + vec4<f32>(1.0 - texel.rgb, 1.0);
}";

/// A scene patch that reads element 2 of a texture array at its own texture coordinate.
pub const ARRAY_PATCH: &str = "
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var layers_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_layer(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(layers, layers_sampler, in.uv, 2);
}";

/// Position (source channel 0) feeds input 0 and the first texture coordinate (source channel
/// 4) feeds input 1.
pub const POSITION_AND_UV: [[i8; 2]; 2] = [[0, 0], [4, 1]];
pub const POSITION: i8 = 0;
pub const UV: i8 = 4;

/// The same inputs as [`POSITION_AND_UV`], numbered as the Unity 6 player's tables number the
/// shader inputs of most variants: 13 more than the SPIR-V location.
pub const POSITION_AND_UV_FROM_13: [[i8; 2]; 2] = [[0, 13], [4, 14]];

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

/// `tint_parameters` and an albedo texture with its own sampler.
pub fn albedo_parameters() -> VariantParameters {
    let mut parameters = tint_parameters();
    parameters.bindings.push(binding(
        "_Albedo",
        slot(0, 2, FRAGMENT),
        ResourceKind::Texture {
            sampler: Some(slot(0, 3, FRAGMENT)),
            dimension: 4,
        },
    ));
    parameters.bindings.push(binding(
        "",
        slot(0, 3, FRAGMENT),
        ResourceKind::Sampler { state: 0 },
    ));
    parameters
}

/// The blit pass's constants and its source texture, which the blit binds under `main`.
pub fn blit_parameters(main: &str) -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![constant_buffer(
            "Material",
            32,
            vec![
                vector_member("_Tint", 0),
                vector_member(&format!("{main}_TexelSize"), 16),
            ],
        )],
        bindings: vec![
            binding(
                "Material",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding(
                main,
                slot(0, 1, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 2, FRAGMENT)),
                    dimension: 4,
                },
            ),
            binding("", slot(0, 2, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    }
}

/// The patch's texture array, which a scene draw binds, with its sampler.
pub fn array_patch_parameters() -> VariantParameters {
    VariantParameters {
        constant_buffers: Vec::new(),
        bindings: vec![
            binding(
                "_Layers",
                slot(0, 0, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 1, FRAGMENT)),
                    dimension: 10,
                },
            ),
            binding("", slot(0, 1, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    }
}

pub fn variant(
    keywords: &[&'static str],
    wgsl: &str,
    fragment: &'static str,
    parameters: VariantParameters,
) -> VariantSpec {
    VariantSpec {
        keywords: keywords.to_vec(),
        wgsl: wgsl.to_owned(),
        entries: ["vs_main", fragment],
        parameters,
        channels: POSITION_AND_UV.to_vec(),
    }
}

pub fn tint_variant(keywords: &[&'static str], fragment: &'static str) -> VariantSpec {
    variant(keywords, TINT, fragment, tint_parameters())
}

/// Turns culling off in every pass, so the tests do not depend on a quad's winding.
pub fn no_cull(mut synthetic: SyntheticShader) -> SyntheticShader {
    for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
        pass.state.depth.cull = fixed(0.0);
    }
    synthetic
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

/// One pass tagged `Alpha` that tints, whose vertex input table is `channels`.
pub fn camera_pass_with_table(channels: &[[i8; 2]]) -> SyntheticShader {
    no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![VariantSpec {
                channels: channels.to_vec(),
                ..tint_variant(&[], "fs_main")
            }],
        }],
    ))
}

/// One pass tagged `Alpha` that tints, and one tagged `Gamma` that samples the albedo texture.
pub fn camera_passes() -> SyntheticShader {
    no_cull(shader(
        &[],
        &[
            PassSpec {
                tags: vec![("LIGHTMODE", "Alpha")],
                variants: vec![tint_variant(&[], "fs_main")],
            },
            PassSpec {
                tags: vec![("LIGHTMODE", "Gamma")],
                variants: vec![variant(&[], ALBEDO, "fs_shifted", albedo_parameters())],
            },
        ],
    ))
}

/// One pass that draws the array patch.
pub fn array_patch() -> SyntheticShader {
    no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                ARRAY_PATCH,
                "fs_layer",
                array_patch_parameters(),
            )],
        }],
    ))
}

/// Blit passes: 0 multiplies the source by the tint, 1 shows the texel size, 2 inverts. The
/// source is the texture property `main`.
pub fn blit_passes(main: &str) -> SyntheticShader {
    no_cull(shader(
        &[],
        &["fs_main", "fs_texel", "fs_inverted"].map(|fragment| PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], BLIT, fragment, blit_parameters(main))],
        }),
    ))
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// A quad over clip space with its texture coordinates, as two triangles of an indexed mesh at
/// depth `z`. Row 0 of the image is clip y of -1, so the texture coordinate is the image's own.
pub fn quad(z: f32) -> Mesh {
    let corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    let positions: Vec<f32> = corners.iter().flat_map(|&(x, y)| [x, y, z]).collect();
    let uvs: Vec<f32> = corners
        .iter()
        .flat_map(|&(x, y)| [f32::midpoint(x, 1.0), f32::midpoint(y, 1.0)])
        .collect();
    Mesh {
        vertex_count: 4,
        channels: vec![
            MeshChannel {
                source: POSITION,
                format: ChannelFormat::Float32x3,
                bytes: floats(&positions),
            },
            MeshChannel {
                source: UV,
                format: ChannelFormat::Float32x2,
                bytes: floats(&uvs),
            },
        ],
        indices: MeshIndices::U16(vec![0, 1, 2, 2, 1, 3]),
        sub_meshes: vec![SubMesh {
            first: 0,
            count: 6,
            base_vertex: 0,
        }],
    }
}

/// The same quad as six vertices with no indices, which is what a blit draws.
pub fn blit_quad() -> Mesh {
    let corners = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (1.0, 1.0),
    ];
    let positions: Vec<f32> = corners.iter().flat_map(|&(x, y)| [x, y, 0.0]).collect();
    let uvs: Vec<f32> = corners
        .iter()
        .flat_map(|&(x, y)| [f32::midpoint(x, 1.0), f32::midpoint(y, 1.0)])
        .collect();
    Mesh {
        vertex_count: 6,
        channels: vec![
            MeshChannel {
                source: POSITION,
                format: ChannelFormat::Float32x3,
                bytes: floats(&positions),
            },
            MeshChannel {
                source: UV,
                format: ChannelFormat::Float32x2,
                bytes: floats(&uvs),
            },
        ],
        indices: MeshIndices::None,
        sub_meshes: vec![SubMesh {
            first: 0,
            count: 6,
            base_vertex: 0,
        }],
    }
}
