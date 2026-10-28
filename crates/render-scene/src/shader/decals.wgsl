struct SceneDecalVolume {
    local_from_world: mat4x4<f32>,
    direction_edge: vec4<f32>,
    surface_angle: vec4<f32>,
    selection: vec4<u32>,
    reserved: vec4<f32>,
    uv_transform: vec4<f32>,
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
    for (var i = 0u; i < scene_decals.count.x; i++) {
        let decal = scene_decals.volumes[i];
        if (decal.selection.x & receiver) == 0u { continue; }
        let local = (decal.local_from_world * vec4<f32>(position, 1.0)).xyz;
        let edge = 0.5 - max(max(abs(local.x), abs(local.y)), abs(local.z));
        if edge < 0.0 { continue; }
        var weight = scene_decal_angle(dot(geometric, decal.direction_edge.xyz), decal.surface_angle.zw, decal.reserved.yzw);
        if weight == 0.0 { continue; }
        let angle_weight = weight;
        if decal.direction_edge.w > 0.0 { weight *= clamp(edge / decal.direction_edge.w, 0.0, 1.0); }
        let uv = (scene_decal_uv(local, decal.selection.z) + 0.5) * decal.uv_transform.xy + decal.uv_transform.zw;
        let dx = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dx, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy;
        let dy = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dy, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy;
        let sample = textureSampleGrad(scene_decal_albedo, scene_decal_sampler, uv, i32(decal.selection.y), dx, dy);
        if (decal.selection.x & 4194304u) != 0u {
            let cutoff = f32((decal.selection.x >> 2u) & 1048575u) / 1000000.0;
            if angle_weight * sample.a < cutoff { continue; }
        }
        weight *= sample.a * decal.reserved.x;
        if (decal.selection.w & 65536u) == 0u {
            result.albedo = mix(result.albedo, sample.rgb, weight);
        }
        var surface = decal.surface_angle.xy;
        let surface_layer = (decal.selection.w >> 8u) & 255u;
        if surface_layer != 0u {
            surface = textureSampleGrad(scene_decal_surface, scene_decal_sampler, uv, i32(surface_layer - 1u), dx, dy).rb;
        }
        if (decal.selection.w & 262144u) == 0u {
            result.properties = mix(result.properties, surface, weight);
        }
        let normal_layer = decal.selection.w & 255u;
        if normal_layer != 0u && weight > 0.0 && (decal.selection.w & 131072u) == 0u {
            let encoded = textureSampleGrad(scene_decal_normal, scene_decal_sampler, uv, i32(normal_layer - 1u), dx, dy).rg;
            let detail = scene_decal_correct_normal(decal, geometric, encoded);
            result.normal = scene_decal_blend_normal(result.normal, detail, weight);
        }
    }
    return result;
}
