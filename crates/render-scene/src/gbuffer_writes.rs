//! The producer side of the deferred G-buffer: the stock terrain and object sources whose fragment
//! stage stores the surface material in the four G-buffer targets instead of lighting it, and the
//! pipelines used by observation and by the early depth-writing material pass.

mod early;
pub(crate) use early::EarlyOutputs;

use crate::gbuffer::GBUFFER_WGSL;
use crate::renderer::ScenePipelines;
use crate::shader::rewrite::ShaderRewrite;

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
    format!(
        "{}\n{CAMERA}\n{GBUFFER_WGSL}\n{TERRAIN_ENTRY}",
        without_entry(stock, FORWARD_TERRAIN)
    )
}

/// `source` without the fragment entry that starts with `entry`, up to its closing brace at the
/// start of a line. The entry must occur exactly once.
fn without_entry(source: &str, entry: &str) -> String {
    // Replacing the entry's header with itself checks that it occurs exactly once.
    let source = source.anchored("gbuffer_writes forward entry", 1, entry, entry);
    let start = source.find(entry).expect("forward entry");
    let length = source[start..]
        .find("\n}\n")
        .expect("end of the forward entry");
    let end = start + length + "\n}\n".len();
    format!("{}{}", &source[..start], &source[end..])
}

/// The object source whose opaque, masked and relief entries write the G-buffer from
/// `scene_object_gbuffer_material`. The forward entries are removed, as for terrain: their
/// `@builtin` inputs cannot survive as ordinary functions, and the G-buffer entries take their
/// names.
pub(crate) fn object_source(stock: &str) -> String {
    let mut source = stock.to_owned();
    for entry in FORWARD_OBJECTS {
        source = without_entry(&source, &format!("@fragment\nfn {entry}("));
    }
    let card = card_material(stock);
    let preparation = crate::scene_metadata::receiver::prepared_normal_source(stock);
    let material = object_material(stock);
    format!(
        "{source}\n{card}\n{CAMERA}\n{GBUFFER_WGSL}\n{preparation}\n{material}\n{PROJECTED_MATERIAL}\n{OBJECT_ENTRIES}"
    )
}

/// `shade_material` as the G-buffer's object material. Its statements are the stock ones. The
/// derivative operands and the base alpha come first, geometry, mask and visibility reject before
/// any optional term, and the optional terms read the prepared operands.
const OBJECT_MATERIAL: [(&str, &str); 3] = [
    (
        "fn shade_material(original: VertexOutput, front_facing: bool, position: vec3<f32>) -> SceneObjectMaterial {",
        "fn scene_object_gbuffer_material(original: VertexOutput, front_facing: bool, position: vec3<f32>, masked: bool, visible: bool) -> SceneObjectMaterial {",
    ),
    (
        "    var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input, albedo_footprint), input.tint);\n    let surface = scene_surface_channels(input);\n    let auxiliary = scene_auxiliary_samples(input);\n\n    // A double-sided card is one sheet of geometry standing in for a leaf with\n    // two faces. Its stored normal describes the front, so the back must be\n    // flipped or the leaf lights as though the sun were behind it.\n    let geometric_normal = normalize(input.world_normal) * select(-1.0, 1.0, front_facing);\n\n    let mapped_normal = sample_normal(input, geometric_normal, input.world_position, input.world_tangent);\n",
        "    // A double-sided card is one sheet of geometry standing in for a leaf with\n    // two faces. Its stored normal describes the front, so the back must be\n    // flipped or the leaf lights as though the sun were behind it.\n    let geometric_normal = normalize(input.world_normal) * select(-1.0, 1.0, front_facing);\n    let surface_footprint = scene_prepare_surface_channels(input);\n    let normal_preparation = scene_prepare_normal(input, geometric_normal, input.world_position);\n    let decal_gradients = scene_prepare_decals(input.world_position);\n    let occlusion_footprint = scene_prepare_card_occlusion(input);\n    let emission_footprint = scene_prepare_card_emission(input);\n    var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input, albedo_footprint), input.tint);\n    if !visible || !scene_geometry_covered(input) || (masked && !scene_alpha_covered(albedo_sample.a, input)) { discard; }\n    let surface = scene_surface_channels_prepared(input, surface_footprint);\n    let occlusion_sample = scene_occlusion_sample_prepared(input, occlusion_footprint);\n    let emission_sample = scene_emission_sample_prepared(input, emission_footprint);\n    let auxiliary = vec2<f32>(occlusion_sample.x, select(emission_sample, occlusion_sample.y, scene_packed_atlas(input)));\n\n    let mapped_normal = sample_normal_prepared(input, geometric_normal, input.world_tangent, normal_preparation);\n",
    ),
    (
        "    return scene_object_material(input, geometric_normal, surface_material, scattering.y, auxiliary.y);",
        "    return scene_gbuffer_projected_material(input, geometric_normal, surface_material, scattering.y, auxiliary.y, decal_gradients);",
    ),
];

/// The stock `shade_material` body as the G-buffer material; see [`OBJECT_MATERIAL`].
fn object_material(stock: &str) -> String {
    let mut material =
        crate::scene_metadata::receiver::function_source(stock, "shade_material").to_owned();
    for (from, to) in OBJECT_MATERIAL {
        material = material.anchored("gbuffer_writes object material", 1, from, to);
    }
    material
}

/// `scene_object_material`'s decal projection, reading the gradients that were taken before the
/// rejection so that no derivative follows a discard.
const PROJECTED_MATERIAL: &str = "fn scene_gbuffer_projected_material(input: VertexOutput, geometric_normal: vec3<f32>, surface: SceneShadingSurface, masked_amount: f32, emission: f32, gradients: SceneDecalGradients) -> SceneObjectMaterial {
    let projected = scene_project_decals_prepared(input.world_position, geometric_normal, 1u,
        surface.albedo.rgb, surface.properties.xy, surface.normal, gradients);
    let decorated = SceneShadingSurface(vec4<f32>(projected.albedo, surface.albedo.a), projected.normal,
        vec3<f32>(projected.properties, surface.properties.z));
    return SceneObjectMaterial(geometric_normal, decorated, masked_amount, emission);
}";

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
        card = card.anchored("gbuffer_writes card material", 1, from, to);
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
        "G-buffer source anchor {from:?}"
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

/// The early material producer and its camera group. Orchestration installs these only for
/// Render and uses `producer_depth` on the actual scene depth before drawing.
pub(crate) fn early_pipelines(
    device: &wgpu::Device,
    camera: &wgpu::Buffer,
    outputs: EarlyOutputs,
) -> (ScenePipelines, wgpu::BindGroup) {
    crate::scene_metadata::pipelines_from_sources(device, camera, &outputs.sources())
}

/// The ordered formats of either producer source. Observation keeps its existing outputs.
pub(crate) fn source_formats(source: &str) -> Vec<wgpu::TextureFormat> {
    if let Some(outputs) = EarlyOutputs::from_source(source) {
        return outputs.formats();
    }
    let mut formats = crate::gbuffer::GBUFFER_FORMATS.to_vec();
    if source.contains(crate::velocity::MARKER) {
        formats.push(worldrunner_render_post::VELOCITY_FORMAT);
    }
    formats
}

/// Observation tests completed depth without writing. Early material production owns winners
/// and, on shared depth, their stencil marks for later read-only consumers.
pub(crate) fn producer_depth(
    early: Option<EarlyOutputs>,
    format: wgpu::TextureFormat,
) -> crate::renderer::depth::DepthMode {
    use crate::renderer::depth::DepthMode;
    if early.is_none() {
        DepthMode::Reuse
    } else if format.has_stencil_aspect() {
        DepthMode::Prepass
    } else {
        DepthMode::Independent
    }
}

#[cfg(test)]
mod tests;
