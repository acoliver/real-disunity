struct SceneDecalVolume {
    local_from_world: mat4x4<f32>,
    direction_edge: vec4<f32>,
    surface_angle: vec4<f32>,
    selection: vec4<u32>,
    reserved: vec4<f32>,
    uv_transform: vec4<f32>,
    color_modifier: vec4<f32>,
}
struct SceneDecalList {
    count: vec4<u32>,
    volumes: array<SceneDecalVolume, 32>,
}
@group(0) @binding(1) var<uniform> scene_decals: SceneDecalList;
@group(0) @binding(2) var scene_decal_albedo: texture_2d_array<f32>;
@group(0) @binding(3) var scene_decal_sampler: sampler;
@group(0) @binding(4) var scene_decal_normal: texture_2d_array<f32>;
@group(0) @binding(5) var scene_decal_surface: texture_2d_array<f32>;
struct SceneDecalSurface {
    albedo: vec3<f32>,
    properties: vec2<f32>,
    normal: vec3<f32>,
}
fn scene_decal_color(sample: vec4<f32>, modifier: vec4<f32>, edge_mode: bool) -> vec4<f32> {
    var alpha = sample.a * modifier.a;
    if edge_mode {
        // At zero opacity both smoothstep endpoints coincide at one.
        alpha = 0.0;
        if modifier.a > 0.0 {
            let lower = 1.0 - modifier.a;
            alpha = smoothstep(max(lower, 0.0001), min(lower + 0.05, 1.0), sample.a);
        }
    }
    return vec4<f32>(sample.rgb * modifier.rgb, alpha);
}
fn scene_decal_angle(cosine: f32, linear: vec2<f32>, power: vec3<f32>) -> f32 {
    let angle = select(max(0.0, cosine), abs(cosine), power.z != 0.0);
    if power.y > 0.0 { return pow(min(angle / power.x, 1.0), power.y); }
    if angle <= linear.x { return 0.0; }
    if linear.y > 0.0 { return clamp((angle - linear.x) / linear.y, 0.0, 1.0); }
    return 1.0;
}
fn scene_decal_uv(local: vec3<f32>, axis: u32) -> vec2<f32> {
    if axis == 0u { return local.yz; }
    if axis == 1u { return local.zx; }
    return local.xy;
}
fn scene_decal_decode(encoded: vec2<f32>) -> vec3<f32> {
    let xy = encoded * 2.0 - 1.0;
    return vec3<f32>(xy.y, -xy.x, sqrt(max(0.0, 1.0 - dot(xy, xy))));
}
fn scene_decal_blend_normal(receiver: vec3<f32>, decal: vec3<f32>, weight: f32) -> vec3<f32> {
    if weight == 0.0 { return receiver; }
    // Folding carries shading magnitude through both forward and metadata passes.
    let magnitude = mix(length(receiver), length(decal), weight);
    return normalize(mix(receiver, decal, weight)) * magnitude;
}
fn scene_decal_correct(surface: vec3<f32>, detail: vec3<f32>) -> vec3<f32> {
    // Projection retains its near-pole fixed half turn.
    if surface.z < -0.999999 { return vec3<f32>(detail.x, -detail.y, -detail.z); }
    return scene_decal_correct_basis(surface, detail);
}
fn scene_decal_correct_basis(surface: vec3<f32>, detail: vec3<f32>) -> vec3<f32> {
    let a = 1.0 / (1.0 + surface.z);
    let b = -surface.x * surface.y * a;
    let x = vec3<f32>(1.0 - surface.x * surface.x * a, b, -surface.x);
    let y = vec3<f32>(b, 1.0 - surface.y * surface.y * a, -surface.y);
    return x * detail.x + y * detail.y + surface * detail.z;
}
fn scene_decal_basis(decal: SceneDecalVolume) -> mat3x3<f32> {
    let inverse = transpose(mat3x3<f32>(decal.local_from_world[0].xyz,
        decal.local_from_world[1].xyz, decal.local_from_world[2].xyz));
    let x = normalize(inverse[0]);
    let y = normalize(inverse[1]);
    let z = normalize(inverse[2]);
    if decal.selection.z == 0u { return mat3x3<f32>(y, z, x); }
    if decal.selection.z == 1u { return mat3x3<f32>(z, x, y); }
    return mat3x3<f32>(x, y, z);
}
fn scene_decal_correct_normal(decal: SceneDecalVolume, geometric: vec3<f32>, encoded: vec2<f32>) -> vec3<f32> {
    if (decal.selection.w & 524288u) != 0u {
        let detail = scene_decal_decode(encoded);
        // Only the exact south pole is singular; choose a fixed X-axis half turn.
        if geometric.z == -1.0 { return vec3<f32>(detail.x, -detail.y, -detail.z); }
        return scene_decal_correct_basis(geometric, detail);
    }
    let basis = scene_decal_basis(decal);
    let detail = scene_decal_correct(transpose(basis) * geometric, scene_decal_decode(encoded));
    return basis * detail;
}
fn scene_project_decals(position: vec3<f32>, geometric: vec3<f32>, receiver: u32,
    albedo: vec3<f32>, properties: vec2<f32>, normal: vec3<f32>) -> SceneDecalSurface {
    var result = SceneDecalSurface(albedo, properties, normal);
    let world_dx = dpdx(position);
    let world_dy = dpdy(position);
