//! The producer side of the deferred G-buffer: the stock terrain and object sources whose fragment
//! stage stores the surface material in the four G-buffer targets instead of lighting it, and the
//! pipelines the scene draws into the `OpaqueGbuffer` slot with.

use crate::gbuffer::GBUFFER_WGSL;
use crate::renderer::ScenePipelines;

const CAMERA: &str = include_str!("ssao/camera.wgsl");
const TERRAIN_ENTRY: &str = include_str!("gbuffer_writes/terrain.wgsl");
const OBJECT_ENTRIES: &str = include_str!("gbuffer_writes/objects.wgsl");

/// Present in every G-buffer producer source; `ScenePipelines` builds the four G-buffer colour
/// targets for a source that contains it.
pub(crate) const MARKER: &str = "struct SceneGbufferTargets";

/// The forward terrain entry, which the G-buffer terrain source replaces.
const FORWARD_TERRAIN: &str =
    "@fragment\nfn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {";

/// The forward object entries, which the G-buffer object source replaces.
const FORWARD_OBJECTS: [&str; 4] = [
    "fs_relief_opaque",
    "fs_relief_masked",
    "fs_opaque",
    "fs_masked",
];

/// The terrain base source whose `fs_terrain` writes the G-buffer.
///
/// The forward entry is removed rather than renamed, so the module holds no lighting work, and
/// the G-buffer entry keeps the name the terrain program selects. The result is a base source:
/// the terrain program composes its page and layered variants over it as over the stock source.
pub(crate) fn terrain_source(stock: &str) -> String {
    assert_eq!(
        stock.matches(FORWARD_TERRAIN).count(),
        1,
        "the stock terrain source has one forward entry"
    );
    format!(
        "{}\n{CAMERA}\n{GBUFFER_WGSL}\n{TERRAIN_ENTRY}",
        without_entry(stock, FORWARD_TERRAIN)
    )
}

/// `source` without the fragment entry that starts with `entry`, up to its closing brace at the
/// start of a line. The entry must occur exactly once.
fn without_entry(source: &str, entry: &str) -> String {
    assert_eq!(
        source.matches(entry).count(),
        1,
        "one forward entry {entry:?}"
    );
    let start = source.find(entry).expect("forward entry");
    let length = source[start..]
        .find("\n}\n")
        .expect("end of the forward entry");
    let end = start + length + "\n}\n".len();
    format!("{}{}", &source[..start], &source[end..])
}

/// The object source whose opaque, masked and relief entries write the G-buffer from
/// `shade_material`. The forward entries are removed, as for terrain: their `@builtin` inputs
/// cannot survive as ordinary functions, and the G-buffer entries take their names.
pub(crate) fn object_source(stock: &str) -> String {
    let mut source = stock.to_owned();
    for entry in FORWARD_OBJECTS {
        source = without_entry(&source, &format!("@fragment\nfn {entry}("));
    }
    let card = card_material(stock);
    format!("{source}\n{card}\n{CAMERA}\n{GBUFFER_WGSL}\n{OBJECT_ENTRIES}")
}

/// The card shading entry whose material the G-buffer card entries store.
const CARD_SHADE: &str = "fn scene_card_shade(original: VertexOutput, front_facing: bool, \
    position: vec3<f32>, masked: bool, visible: bool) -> vec4<f32> {";

/// The rewrites that turn the frozen `scene_card_shade` into `scene_card_gbuffer_material`: the
/// same statements in the same order up to the material, without the two card shadow
/// preparations and the lighting.
const CARD_MATERIAL: [(&str, &str); 4] = [
    (
        CARD_SHADE,
        "fn scene_card_gbuffer_material(original: VertexOutput, front_facing: bool, \
         position: vec3<f32>, masked: bool, visible: bool) -> SceneObjectMaterial {",
    ),
    (
        "    let front_shadow = scene_prepare_card_shadow(input.world_position, projected.normal);\n",
        "",
    ),
    (
        "    var back_shadow: SceneCardShadow;\n    if bitcast<f32>(material.geometry_flags.y) != 0.0 \
         && frame.grading.y == 0.0 {\n        back_shadow = scene_prepare_card_shadow(\
         input.world_position, -projected.normal);\n    }\n",
        "",
    ),
    (
        "    let shaded = scene_card_finish_shading(input, surface_material, scattering.y, \
         auxiliary.y, front_shadow, back_shadow);\n    return vec4<f32>(shaded.rgb, \
         select(1.0, coverage_alpha, masked));",
        "    return SceneObjectMaterial(geometric_normal, surface_material, scattering.y, auxiliary.y);",
    ),
];

/// `scene_card_gbuffer_material`, derived from the stock card shading so that forward and the
/// G-buffer evaluate one card material. Every rewrite must match exactly once.
fn card_material(stock: &str) -> String {
    let mut card =
        crate::scene_metadata::receiver::function_source(stock, "scene_card_shade").to_owned();
    for (from, to) in CARD_MATERIAL {
        assert_eq!(
            card.matches(from).count(),
            1,
            "card material anchor {from:?}"
        );
        card = card.replace(from, to);
    }
    card
}

/// The (object, terrain) G-buffer sources over the stock sources; with `velocity`, entries that
/// also write the object-motion residual at `@location(4)`.
pub(crate) fn sources(velocity: bool) -> (String, String) {
    if velocity {
        return (
            object_velocity_source(&object_source(&crate::velocity::vertex_source(
                crate::shader::ENVIRONMENT_WGSL,
            ))),
            terrain_velocity_source(&terrain_source(crate::shader::TERRAIN_WGSL)),
        );
    }
    (
        object_source(crate::shader::ENVIRONMENT_WGSL),
        terrain_source(crate::shader::TERRAIN_WGSL),
    )
}

/// The G-buffer targets followed by the residual, the velocity layout of a frame that renders
/// from the G-buffer: the producers draw the surfaces Opaque no longer draws, so they write the
/// residual the forward entries would have. Its name is the velocity marker, which gives the
/// pipelines the fifth target and the previous-transform vertex buffer.
const VELOCITY_OUTPUTS: &str = "
struct SceneVelocityOutput { @location(0) g0: vec4<f32>, @location(1) g1: vec4<f32>, @location(2) g2: vec4<f32>, @location(3) g3: u32, @location(4) velocity: vec2<f32>, }
struct SceneVelocityReliefOutput { @location(0) g0: vec4<f32>, @location(1) g1: vec4<f32>, @location(2) g2: vec4<f32>, @location(3) g3: u32, @builtin(frag_depth) depth: f32, @location(4) velocity: vec2<f32>, }
";

/// Replaces `from` by `to`, which must match exactly once.
fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "G-buffer velocity anchor {from:?}"
    );
    source.replace(from, to)
}

/// `source`, an object G-buffer source over the velocity vertex stage, with entries that add the
/// residual of [`crate::velocity`] to the stored targets.
fn object_velocity_source(source: &str) -> String {
    let mut source = source.to_owned();
    for (entry, output) in [
        ("fs_opaque", "SceneGbufferTargets"),
        ("fs_masked", "SceneGbufferTargets"),
        ("fs_relief_opaque", "SceneGbufferReliefTargets"),
        ("fs_relief_masked", "SceneGbufferReliefTargets"),
    ] {
        source = replace_once(
            &source,
            &format!(
                "@fragment\nfn {entry}(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> {output} {{"
            ),
            &format!("fn stored_{entry}(input: VertexOutput, front_facing: bool) -> {output} {{"),
        );
    }
    format!(
        "{source}\n{VELOCITY_OUTPUTS}{}
@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ let t = stored_fs_opaque(input, front); return SceneVelocityOutput(t.g0, t.g1, t.g2, t.g3, scene_object_velocity(input)); }}
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ let t = stored_fs_masked(input, front); return SceneVelocityOutput(t.g0, t.g1, t.g2, t.g3, scene_object_velocity(input)); }}
@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let t = stored_fs_relief_opaque(input, front); return SceneVelocityReliefOutput(t.g0, t.g1, t.g2, t.g3, t.depth, scene_object_velocity(input)); }}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let t = stored_fs_relief_masked(input, front); return SceneVelocityReliefOutput(t.g0, t.g1, t.g2, t.g3, t.depth, scene_object_velocity(input)); }}
",
        crate::velocity::OBJECT_RESIDUAL
    )
}

/// `source`, the terrain G-buffer source, writing a zero residual: terrain is static.
fn terrain_velocity_source(source: &str) -> String {
    let source = replace_once(
        source,
        "@fragment\nfn fs_terrain(input: TerrainVertexOutput) -> SceneGbufferTargets {",
        "fn stored_terrain(input: TerrainVertexOutput) -> SceneGbufferTargets {",
    );
    format!(
        "{source}\n{VELOCITY_OUTPUTS}
@fragment fn fs_terrain(input: TerrainVertexOutput) -> SceneVelocityOutput {{ let t = stored_terrain(input); return SceneVelocityOutput(t.g0, t.g1, t.g2, t.g3, vec2<f32>(0.0)); }}
"
    )
}

/// The scene's G-buffer pipelines and their screen camera group over `camera`, the screen camera
/// uniform of the frame's prepass metadata; with `velocity`, pipelines that also write the
/// object-motion residual.
pub(crate) fn pipelines(
    device: &wgpu::Device,
    camera: &wgpu::Buffer,
    velocity: bool,
) -> (ScenePipelines, wgpu::BindGroup) {
    crate::scene_metadata::pipelines_from_sources(device, camera, &sources(velocity))
}

#[cfg(test)]
mod tests;
