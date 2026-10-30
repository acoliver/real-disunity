//! Object-motion residual for the TAA resolve, written by the main pass.
//!
//! The residual of a fragment is `uv(P_prev * (w + delta)) - uv(P_prev * w)` where `w` is its
//! deformed world position, `delta` the vertex stage's `previous - current` world motion (the
//! previous model matrix and previous clock against the current ones) and
//! `P_prev` the previous unjittered view-projection. Both terms use the same matrix, so jitter
//! and camera motion cancel and a static fragment stores exactly zero.
//!
//! Velocity-writing pipelines are produced by rewriting the stock fragment entry points, in
//! the style of `ssr::forward`. Each rewrite is guarded by an exact match count so a change to
//! the stock source fails loudly instead of silently dropping the attachment.
//!
//! The object vertex output already fills all 16 inter-stage slots of a default device (15
//! locations plus `front_facing`), so the delta shares the interpolated `ambient_occlusion`
//! location: `ao_delta` holds the occlusion in `x` and the world delta in `yzw`.
//!
//! | Draw                                  | Velocity attachment                       |
//! | ------------------------------------- | ----------------------------------------- |
//! | Opaque, masked and relief objects     | Residual (`@location(1)`): wind and the   |
//! |                                       | instance's own previous transform         |
//! | Billboards and impostor cards         | Wind, instance motion and the turn of the |
//! |                                       | camera-facing basis (previous camera)     |
//! | Terrain                               | Written as zero (static in world space)   |
//! | Dynamic quads                         | Not written (empty mask), stays at zero   |
//! | Sky, celestial mesh composition       | Not written (empty mask), stays at zero   |
//! | Celestial mesh geometry, water        | Own passes, no velocity attachment        |
//! | Decals                                | Part of the receiving object's draw       |
//! | Shadow receivers (objects, terrain)   | Same rewrite as the unshadowed draws      |
//! | SSAO objects and terrain              | Same rewrite, `with_ssao_taa_velocity`;   |
//! |                                       | the SSAO metadata prepass writes none     |
//! | Shadow caster depth draws             | No colour target, nothing to write        |
//! | Reflection forward draws              | Residual at `@location(3)` when it fits   |
//!
//! Reflection forward draws already write three `Rgba16Float` targets, `@location(0..2)`. The
//! residual is a fourth target, so the colour budget per sample is three colour formats plus
//! [`worldrunner_render_post::VELOCITY_FORMAT`] (28 bytes for HDR against the default limit of
//! 32). [`reflection_velocity`] decides from the device limit; a device that cannot hold the
//! extra target keeps the reflection draws on camera reprojection (the resolve reads zeros).
//!
//! Pipelines that write velocity carry [`MARKER`] in their source; it is how the target list
//! learns about the second colour target.

/// Whether reflection forward draws can also write the residual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReflectionVelocity {
    /// The residual is written at `@location(3)` beside the three reflection targets.
    Written,
    /// The colour budget cannot hold the residual; the resolve reprojects with the camera only.
    CameraOnly,
}

/// The reflection forward draws' rule: `colour` is the scene colour format, `limit` the device's
/// `max_color_attachment_bytes_per_sample`.
///
/// The three reflection targets (colour, environment, factor) all use `colour`. An unknown cost
/// for either format selects [`ReflectionVelocity::CameraOnly`].
pub(crate) fn reflection_velocity(colour: wgpu::TextureFormat, limit: u32) -> ReflectionVelocity {
    let cost = colour
        .target_pixel_byte_cost()
        .zip(worldrunner_render_post::VELOCITY_FORMAT.target_pixel_byte_cost())
        .and_then(|(colour, velocity)| colour.checked_mul(3)?.checked_add(velocity));
    match cost {
        Some(cost) if cost <= limit => ReflectionVelocity::Written,
        _ => ReflectionVelocity::CameraOnly,
    }
}

/// Whether reflection forward draws write the residual on `device`, given that `requested`.
pub(crate) fn reflection_writes_velocity(
    device: &wgpu::Device,
    colour: wgpu::TextureFormat,
    requested: bool,
) -> bool {
    requested
        && reflection_velocity(
            colour,
            device.limits().max_color_attachment_bytes_per_sample,
        ) == ReflectionVelocity::Written
}

/// Present in the source of every pipeline set that writes the velocity target.
pub(crate) const MARKER: &str = "struct SceneVelocityOutput";

const OUTPUT: &str = "
struct SceneVelocityOutput { @location(0) colour: vec4<f32>, @location(1) velocity: vec2<f32>, }
struct SceneVelocityReliefOutput { @location(0) colour: vec4<f32>, @builtin(frag_depth) depth: f32, @location(1) velocity: vec2<f32>, }
";

const OBJECT_RESIDUAL: &str = "
fn scene_object_velocity(input: VertexOutput) -> vec2<f32> {
    let current = previous_frame.view_projection * vec4<f32>(input.world_position, 1.0);
    let before = previous_frame.view_projection * vec4<f32>(input.world_position + input.ao_delta.yzw, 1.0);
    if current.w <= 0.0 || before.w <= 0.0 { return vec2<f32>(0.0); }
    let motion = before.xy / before.w - current.xy / current.w;
    return vec2<f32>(0.5 * motion.x, -0.5 * motion.y);
}
";

fn replace_times(source: &str, from: &str, to: &str, times: usize) -> String {
    assert_eq!(
        source.matches(from).count(),
        times,
        "velocity rewrite target changed: {from}"
    );
    source.replace(from, to)
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    replace_times(source, from, to, 1)
}

/// Declarations behind the previous-transform vertex buffer (locations 12 to 15, the layout of
/// the instance buffer's matrix). The model matrix is handed to the delta through a private
/// variable so the stock vertex helpers keep their signatures.
///
/// Billboards and impostor cards build their basis from the camera, so their previous position
/// is rebuilt by `scene_card_position` from the previous camera (`previous_frame.camera_time.xyz`)
/// and the previous pivot and scale, through the same `billboard_basis_from` the stock vertex
/// stage uses for the current frame. The card vertex `scene_card_local` is the same local offset
/// both times: the authored vertex, or the cropped plane point of a relief impostor. The delta is
/// added to the stock position as `previous card - current card`, so an unchanged camera and
/// instance give bitwise equal terms and a delta of exactly zero.
const PREVIOUS_INSTANCE: &str = "
struct PreviousInstance {
    @location(12) model_column_0: vec4<f32>,
    @location(13) model_column_1: vec4<f32>,
    @location(14) model_column_2: vec4<f32>,
    @location(15) model_column_3: vec4<f32>,
}
var<private> scene_previous_model: mat4x4<f32>;
var<private> scene_card_local: vec3<f32>;
fn scene_card_position(camera: vec3<f32>, mode: u32, model: mat4x4<f32>, local: vec3<f32>) -> vec3<f32> {
    var center = model[3].xyz;
    if material.impostor_grid.w != 0.0 {
        center += mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz) * material.wind_field_mode.yzw;
    }
    return center + billboard_basis_from(camera, mode, center) * length(model[0].xyz) * local;
}
fn scene_object_world_delta(vertex: VertexInput, position: vec3<f32>, instance: InstanceInput) -> vec3<f32> {
    var before = instance;
    before.model_column_0 = vec4<f32>(scene_previous_model[0].xyz, instance.model_column_0.w);
    before.model_column_1 = vec4<f32>(scene_previous_model[1].xyz, instance.model_column_1.w);
    before.model_column_2 = vec4<f32>(scene_previous_model[2].xyz, instance.model_column_2.w);
    before.model_column_3 = vec4<f32>(scene_previous_model[3].xyz, instance.model_column_3.w);
    var before_position = position;
    if (vertex.lod_billboard_mode & 3u) == BILLBOARD_FIXED {
        let rotation = mat3x3<f32>(scene_previous_model[0].xyz, scene_previous_model[1].xyz, scene_previous_model[2].xyz);
        before_position = scene_previous_model[3].xyz + rotation * vertex.position;
    } else {
        let mode = vertex.lod_billboard_mode & 3u;
        let current_model = mat4x4<f32>(instance.model_column_0, instance.model_column_1, instance.model_column_2, instance.model_column_3);
        let current_card = scene_card_position(frame.camera_position.xyz, mode, current_model, scene_card_local);
        let previous_card = scene_card_position(previous_frame.camera_time.xyz, mode, scene_previous_model, scene_card_local);
        before_position = position + (previous_card - current_card);
    }
    return scene_world_delta(position, before_position, vertex.ambient_occlusion, instance, before);
}
";

/// The stock `billboard_basis`, now a view of `billboard_basis_from` at the current camera.
const BILLBOARD_BASIS: &str = "
fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> { return billboard_basis_from(frame.camera_position.xyz, mode, pivot); }
";

/// The object source with the previous-position varying, before any fragment rewrite.
///
/// Reflection and screen-space variants apply their own fragment rewrite afterwards, so the
/// vertex stage is rewritten first, on the stock source.
pub(crate) fn vertex_source(source: &str) -> String {
    let source = replace_once(
        source,
        "    @location(4) ambient_occlusion: f32,",
        "    @location(4) ao_delta: vec4<f32>,",
    );
    let source = replace_times(&source, "input.ambient_occlusion", "input.ao_delta.x", 2);
    let source = replace_times(&source, "out.ambient_occlusion", "out.ao_delta.x", 5);
    let source = replace_once(
        &source,
        "    let displacement = scene_geometry_displacement(out.world_position,",
        "    out.ao_delta = vec4<f32>(out.ao_delta.x, scene_object_world_delta(vertex, out.world_position, instance));\n    let displacement = scene_geometry_displacement(out.world_position,",
    );
    let source = replace_once(
        &source,
        "@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {",
        "@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput, previous: PreviousInstance) -> VertexOutput {\n    scene_previous_model = mat4x4<f32>(previous.model_column_0, previous.model_column_1, previous.model_column_2, previous.model_column_3);\n    scene_card_local = vertex.position;",
    );
    let source = replace_once(
        &source,
        "            out.world_position = capture_center + billboard_basis(vertex.lod_billboard_mode & 3u, capture_center) * scale * shaped;",
        "            out.world_position = capture_center + billboard_basis(vertex.lod_billboard_mode & 3u, capture_center) * scale * shaped;\n            scene_card_local = shaped;",
    );
    let source = replace_once(
        &source,
        "fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {\n    let to_camera = frame.camera_position.xyz - pivot;",
        "fn billboard_basis_from(camera: vec3<f32>, mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {\n    let to_camera = camera - pivot;",
    );
    format!("{source}\n{PREVIOUS_INSTANCE}{BILLBOARD_BASIS}")
}

/// The object source with velocity-writing fragment entry points and the `world_delta` varying.
pub(crate) fn object_source(source: &str) -> String {
    let source = vertex_source(source);
    let source = replace_once(
        &source,
        "@fragment\nfn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput",
        "fn shaded_relief_opaque(input: VertexOutput, front_facing: bool) -> SceneSurfaceOutput",
    );
    let source = replace_once(
        &source,
        "@fragment\nfn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput",
        "fn shaded_relief_masked(input: VertexOutput, front_facing: bool) -> SceneSurfaceOutput",
    );
    let source = replace_once(
        &source,
        "@fragment\nfn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32>",
        "fn shaded_opaque(input: VertexOutput, front_facing: bool) -> vec4<f32>",
    );
    let source = replace_once(
        &source,
        "@fragment\nfn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32>",
        "fn shaded_masked(input: VertexOutput, front_facing: bool) -> vec4<f32>",
    );
    format!(
        "{source}\n{OUTPUT}{OBJECT_RESIDUAL}
@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ return SceneVelocityOutput(shaded_opaque(input, front), scene_object_velocity(input)); }}
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ return SceneVelocityOutput(shaded_masked(input, front), scene_object_velocity(input)); }}
@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let shaded = shaded_relief_opaque(input, front); return SceneVelocityReliefOutput(shaded.colour, shaded.depth, scene_object_velocity(input)); }}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let shaded = shaded_relief_masked(input, front); return SceneVelocityReliefOutput(shaded.colour, shaded.depth, scene_object_velocity(input)); }}
"
    )
}

/// The terrain source with a fragment entry point that writes a zero residual.
pub(crate) fn terrain_source(source: &str) -> String {
    let source = replace_once(
        source,
        "@fragment\nfn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32>",
        "fn shaded_terrain(input: TerrainVertexOutput) -> vec4<f32>",
    );
    format!(
        "{source}\n{OUTPUT}
@fragment fn fs_terrain(input: TerrainVertexOutput) -> SceneVelocityOutput {{ return SceneVelocityOutput(shaded_terrain(input), vec2<f32>(0.0)); }}
"
    )
}

const REFLECTION_OUTPUT: &str = "
struct SceneVelocityOutput { @location(0) colour: vec4<f32>, @location(1) environment: vec4<f32>, @location(2) factor: vec4<f32>, @location(3) velocity: vec2<f32>, }
";

/// Reflection forward object source (see `ssr::forward::source`) with the residual at
/// `@location(3)`. `source` must come from a stock source already passed through [`vertex_source`].
pub(crate) fn reflection_object_source(source: &str) -> String {
    let mut source = source.to_owned();
    for (name, output) in [
        ("fs_relief_opaque", "ReflectionOutput"),
        ("fs_relief_masked", "ReflectionOutput"),
        ("fs_opaque", "ReflectionStockOutput"),
        ("fs_masked", "ReflectionStockOutput"),
    ] {
        let from = format!(
            "@fragment fn {name}(input: VertexOutput, @builtin(front_facing) front: bool) -> {output}"
        );
        let to = format!("fn reflected_{name}(input: VertexOutput, front: bool) -> {output}");
        source = replace_once(&source, &from, &to);
    }
    format!(
        "{source}\n{REFLECTION_OUTPUT}{OBJECT_RESIDUAL}
struct SceneVelocityReliefOutput {{ @builtin(frag_depth) depth: f32, @location(0) colour: vec4<f32>, @location(1) environment: vec4<f32>, @location(2) factor: vec4<f32>, @location(3) velocity: vec2<f32>, }}
@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ let r = reflected_fs_opaque(input, front); return SceneVelocityOutput(r.color, r.environment, r.factor, scene_object_velocity(input)); }}
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityOutput {{ let r = reflected_fs_masked(input, front); return SceneVelocityOutput(r.color, r.environment, r.factor, scene_object_velocity(input)); }}
@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let r = reflected_fs_relief_opaque(input, front); return SceneVelocityReliefOutput(r.depth, r.color, r.environment, r.factor, scene_object_velocity(input)); }}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneVelocityReliefOutput {{ let r = reflected_fs_relief_masked(input, front); return SceneVelocityReliefOutput(r.depth, r.color, r.environment, r.factor, scene_object_velocity(input)); }}
"
    )
}

/// Reflection forward terrain source with a zero residual at `@location(3)`.
pub(crate) fn reflection_terrain_source(source: &str) -> String {
    let source = replace_once(
        source,
        "@fragment fn fs_terrain(input: TerrainVertexOutput) -> ReflectionOutput",
        "fn reflected_terrain(input: TerrainVertexOutput) -> ReflectionOutput",
    );
    format!(
        "{source}\n{REFLECTION_OUTPUT}
@fragment fn fs_terrain(input: TerrainVertexOutput) -> SceneVelocityOutput {{ let r = reflected_terrain(input); return SceneVelocityOutput(r.color, r.environment, r.factor, vec2<f32>(0.0)); }}
"
    )
}

/// A velocity object source whose camera-facing previous basis uses the current camera, which
/// is how billboards and impostor cards behaved before the previous camera was tracked.
#[cfg(test)]
pub(crate) fn current_camera_object_source(source: &str) -> String {
    replace_once(
        &object_source(source),
        "scene_card_position(previous_frame.camera_time.xyz,",
        "scene_card_position(frame.camera_position.xyz,",
    )
}

#[cfg(test)]
mod tests {
    use super::{ReflectionVelocity, reflection_velocity};
    use wgpu::TextureFormat::{Rgba16Float, Rgba32Float};

    #[test]
    fn hdr_reflection_targets_plus_velocity_fit_the_default_limit_of_32_bytes() {
        assert_eq!(
            reflection_velocity(Rgba16Float, 32),
            ReflectionVelocity::Written
        );
        assert_eq!(
            reflection_velocity(Rgba16Float, 28),
            ReflectionVelocity::Written
        );
    }

    #[test]
    fn a_limit_below_the_28_byte_cost_keeps_camera_reprojection() {
        assert_eq!(
            reflection_velocity(Rgba16Float, 27),
            ReflectionVelocity::CameraOnly
        );
        assert_eq!(
            reflection_velocity(Rgba16Float, 24),
            ReflectionVelocity::CameraOnly
        );
    }

    #[test]
    fn the_cost_follows_the_colour_format() {
        // 16 bytes per Rgba32Float target: 3 * 16 + 4.
        assert_eq!(
            reflection_velocity(Rgba32Float, 52),
            ReflectionVelocity::Written
        );
        assert_eq!(
            reflection_velocity(Rgba32Float, 51),
            ReflectionVelocity::CameraOnly
        );
        assert_eq!(
            reflection_velocity(Rgba32Float, 32),
            ReflectionVelocity::CameraOnly
        );
    }
}
