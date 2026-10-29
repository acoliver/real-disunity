//! Object-motion residual for the TAA resolve, written by the main pass.
//!
//! The residual of a fragment is `uv(P_prev * (w + delta)) - uv(P_prev * w)` where `w` is its
//! deformed world position, `delta` the vertex stage's `previous - current` world motion and
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
//! | Opaque, masked and relief objects     | Residual (`@location(1)`)                 |
//! | Terrain                               | Written as zero (static in world space)   |
//! | Dynamic quads                         | Not written (empty mask), stays at zero   |
//! | Water, sky, celestial meshes, decals  | Not written, stays at the cleared zero    |
//!
//! Pipelines that write velocity carry [`MARKER`] in their source; it is how the target list
//! learns about the second colour target.

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

/// The object source with velocity-writing fragment entry points and the `world_delta` varying.
pub(crate) fn object_source(source: &str) -> String {
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
        "    out.ao_delta = vec4<f32>(out.ao_delta.x, scene_wind_world_delta(out.world_position, vertex.ambient_occlusion, instance));\n    let displacement = scene_geometry_displacement(out.world_position,",
    );
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
