// Metallic-roughness shading: roughness in green, metalness in blue.
// Normal maps encode tangent-space X/Y; the positive Z component is reconstructed.


// Frame state, the reflectance model and the derivative tangent frame arrive
// from lighting.wgsl, which is prepended to this source.

struct Material {
    alpha_cutoff: f32,
    roughness: f32,
    metalness: f32,
    texture_flags: u32,
    tint_ao: vec4<f32>,
    wind_direction_amplitude: vec4<f32>,
    wind_clock: vec4<u32>,
    geometry_flags: vec4<u32>,
    contact_tint_distance: vec4<f32>,
    contact_surface: vec4<f32>,
}

const FLAG_NORMAL: u32 = 1u;
const FLAG_ROUGH_METAL: u32 = 2u;
const FLAG_AMBIENT_OCCLUSION: u32 = 4u;
const FLAG_SELF_ILLUMINATION: u32 = 8u;

@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var albedo_texture: texture_2d<f32>;
@group(1) @binding(2) var normal_texture: texture_2d<f32>;
@group(1) @binding(3) var rough_metal_texture: texture_2d<f32>;
@group(1) @binding(4) var ambient_occlusion_texture: texture_2d<f32>;
@group(1) @binding(5) var self_illumination_texture: texture_2d<f32>;
@group(1) @binding(6) var material_sampler: sampler;
struct ContactHeights {
    origin_spacing_columns: vec4<f32>,
    rows_reserved: vec4<f32>,
    heights: array<f32>,
}
@group(1) @binding(7) var<storage, read> contact_heights: ContactHeights;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(8) lod_billboard_mode: u32,
    @location(7) ambient_occlusion: f32,
}

struct InstanceInput {
    @location(3) model_column_0: vec4<f32>,
    @location(4) model_column_1: vec4<f32>,
    @location(5) model_column_2: vec4<f32>,
    @location(6) model_column_3: vec4<f32>,
    @location(10) tint: vec4<f32>,
}

// Draw the authored geometry unturned. Crossed quads land here too: they earn
// their approximation by not turning, so there is nothing for this stage to do.
const BILLBOARD_FIXED: u32 = 0u;
// Pivot about the scene's vertical axis only, so the card stays upright.
const BILLBOARD_YAW_LOCKED: u32 = 1u;
// Face the camera on every axis.
const BILLBOARD_SPHERICAL: u32 = 2u;

// Scene space is Z-up: the basis change to Y-up happens at the view level, not
// here. So a billboard pivots about +Z, and this is the axis it stands on.
const SCENE_UP: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);

// Below this the direction being normalised carries no usable direction.
const BILLBOARD_EPSILON: f32 = 1.0e-6;

// How a billboard card must be authored.
//
// The card lies in its local X–Z plane, standing on local +Z, with its outward
// normal along local −Y. The renderer then maps local +Y to the direction
// pointing away from the camera, which makes (right, away, up) right-handed:
// an observer sees local +X to their right and local +Z upward, exactly the
// frame the card was authored in. Choosing local +Y as the outward normal
// instead would make that triple left-handed, and the card would render
// mirrored — a fault that reads as a texturing mistake rather than a basis
// one, which is why the convention is written down here rather than inferred.
fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {
    let to_camera = frame.camera_position.xyz - pivot;

    if mode == BILLBOARD_YAW_LOCKED {
        let horizontal = vec2<f32>(to_camera.x, to_camera.y);
        // A camera directly overhead determines no yaw at all. The answer has
        // to come from somewhere, and it has to be the same answer every
        // frame: a yaw derived from the vanishing horizontal would swing
        // wildly as the camera crossed the zenith, so hold still instead.
        if dot(horizontal, horizontal) < BILLBOARD_EPSILON {
            return mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), SCENE_UP);
        }
        let away = vec3<f32>(-normalize(horizontal), 0.0);
        return mat3x3<f32>(cross(away, SCENE_UP), away, SCENE_UP);
    }

    let away = -normalize(to_camera);
    let sideways = cross(away, SCENE_UP);
    // Looking straight down the vertical axis leaves the card's roll free; any
    // choice is as correct as another, so take a fixed one to stay stable.
    if dot(sideways, sideways) < BILLBOARD_EPSILON {
        let right = vec3<f32>(1.0, 0.0, 0.0);
        return mat3x3<f32>(right, away, cross(right, away));
    }
    let right = normalize(sideways);
    return mat3x3<f32>(right, away, cross(right, away));
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) ambient_occlusion: f32,
    @location(5) @interpolate(flat) coverage: u32,
}

fn transform_vertex(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(
        instance.model_column_0,
        instance.model_column_1,
        instance.model_column_2,
        instance.model_column_3,
    );

    var rotation = mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
    let pivot = model[3].xyz;

    let billboard_mode = vertex.lod_billboard_mode & 3u;
    if billboard_mode != BILLBOARD_FIXED {
        // The authored rotation is replaced, not composed with: a billboard's
        // whole purpose is to face the camera whatever the placement said.
        // Position and scale survive, and scale is one column's length because
        // SceneTransform scales all three axes alike.
        rotation = billboard_basis(billboard_mode, pivot) * length(model[0].xyz);
    }

    let world_position = pivot + rotation * vertex.position;

    var out: VertexOutput;
    out.clip_position = frame.view_projection * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.world_normal = scene_normal(rotation, vertex.normal);
    out.texcoord = vertex.texcoord;
    out.tint = vec4<f32>(1.0);
    out.ambient_occlusion = 1.0;
    out.coverage = vertex.lod_billboard_mode & 0xfffffffcu;
    return out;
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out = transform_vertex(vertex, instance);
    let displacement = scene_wind_offset(material.wind_direction_amplitude, material.wind_clock.xy,
        bitcast<u32>(frame.grading.z));
    if material.wind_clock.x != 0u {
        out.world_position += displacement;
        out.clip_position = frame.view_projection * vec4<f32>(out.world_position, 1.0);
    }
    out.tint = vec4<f32>(instance.tint.rgb * material.tint_ao.rgb, 1.0);
    out.ambient_occlusion = material.tint_ao.a;
    if material.geometry_flags.x != 0u {
        out.ambient_occlusion *= vertex.ambient_occlusion;
    }
    return out;
}

struct InstanceTint {
    @location(9) tint: vec4<f32>,
}

@vertex
fn vs_unlit(vertex: VertexInput, instance: InstanceInput, tint: InstanceTint) -> VertexOutput {
    var out = transform_vertex(vertex, instance);
    out.tint = tint.tint;
    return out;
}


// Terrain depth draws have no material binding and no object deformation.
@vertex
fn vs_terrain_depth(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    return transform_vertex(vertex, instance);
}

// The descriptor specifies a rigid periodic world-space displacement in metres.
// Integer clock reduction keeps long-running frames in the same phase interval.
fn scene_wind_offset(direction_amplitude: vec4<f32>, clock: vec2<u32>, time_ms: u32) -> vec3<f32> {
    if clock.x == 0u {
        return vec3<f32>(0.0);
    }
    let cycles = f32(time_ms % clock.x) / f32(clock.x) + f32(clock.y) / 1000000.0;
    return direction_amplitude.xyz * (direction_amplitude.w * sin(cycles * 2.0 * PI));
}

fn scene_normal(linear: mat3x3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let lengths = vec3<f32>(dot(linear[0], linear[0]), dot(linear[1], linear[1]), dot(linear[2], linear[2]));
    let determinant = dot(linear[0], cross(linear[1], linear[2]));
    // Retain stock arithmetic for the positive uniform-scale scene transform.
    let orthogonal = max(abs(dot(linear[0], linear[1])), max(abs(dot(linear[0], linear[2])), abs(dot(linear[1], linear[2]))));
    if determinant > 0.0 && max(abs(lengths.x - lengths.y), abs(lengths.x - lengths.z)) <= lengths.x * 0.000001 && orthogonal <= lengths.x * 0.000001 {
        return normalize(linear * normal);
    }
    let cofactors = mat3x3<f32>(cross(linear[1], linear[2]), cross(linear[2], linear[0]), cross(linear[0], linear[1]));
    return normalize((cofactors * normal) / determinant);
}

fn scene_alpha_covered(alpha: f32, input: VertexOutput) -> bool {
    return alpha >= material.alpha_cutoff && scene_geometry_covered(input);
}

fn scene_albedo(sampled: vec4<f32>, tint: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(sampled.rgb * tint.rgb, sampled.a);
}

// Both representations use the same raster pixel threshold in every pass.
fn scene_card_covered(pixel: vec2<f32>, coverage: f32) -> bool {
    let cell = bitcast<vec2<u32>>(vec2<i32>(floor(pixel)));
    var hash = cell.x * 1664525u + cell.y * 1013904223u;
    hash = (hash ^ (hash >> 16u)) * 2246822519u;
    let threshold = (f32(hash & 65535u) + 0.5) / 65536.0;
    return coverage >= threshold;
}

fn scene_transmission(amount: f32, backlight: f32) -> f32 {
    return amount * max(backlight, 0.0);
}

fn scene_contact_weight(height_above: f32, distance: f32) -> f32 {
    return 1.0 - clamp(abs(height_above) / distance, 0.0, 1.0);
}

fn scene_contact(position: vec3<f32>) -> f32 {
    if material.contact_tint_distance.w == 0.0 { return 0.0; }
    let grid = contact_heights.origin_spacing_columns;
    let maximum = vec2<f32>(grid.w, contact_heights.rows_reserved.x) - 1.0;
    let point = (position.xy - grid.xy) / grid.z;
    if any(point < vec2<f32>(0.0)) || any(point > maximum) { return 0.0; }
    let stride = contact_heights.rows_reserved.y;
    let lower = min(floor(point / stride), ceil(maximum / stride) - 1.0) * stride;
    let upper = min(lower + stride, maximum);
    let fraction = (point - lower) / (upper - lower);
    let cell = vec2<u32>(floor(lower / stride));
    let next = vec2<u32>(ceil(upper / stride));
    let columns = u32(contact_heights.rows_reserved.z);
    let a = contact_heights.heights[cell.y * columns + cell.x];
    let b = contact_heights.heights[cell.y * columns + next.x];
    let c = contact_heights.heights[next.y * columns + cell.x];
    let d = contact_heights.heights[next.y * columns + next.x];
    var height: f32;
    if fraction.x >= fraction.y {
        height = a + (b - a) * fraction.x + (d - b) * fraction.y;
    } else {
        height = a + (d - c) * fraction.x + (c - a) * fraction.y;
    }
    return scene_contact_weight(position.z - height, material.contact_tint_distance.w);
}

fn scene_contact_albedo(albedo: vec3<f32>, weight: f32) -> vec3<f32> {
    return mix(albedo, material.contact_tint_distance.rgb, weight);
}

fn scene_contact_surface(surface: vec2<f32>, weight: f32) -> vec2<f32> {
    return mix(surface, material.contact_surface.xy, weight);
}

fn scene_geometry_covered(input: VertexOutput) -> bool {
    let coverage = 1.0 - f32((input.coverage & 0x7fffffffu) >> 2u) / 1000000.0;
    let covered = scene_card_covered(input.clip_position.xy, coverage);
    if (input.coverage & 0x80000000u) != 0u { return !covered; }
    return covered;
}

fn scene_apply_coverage(input: VertexOutput) {
    if !scene_geometry_covered(input) { discard; }
}
