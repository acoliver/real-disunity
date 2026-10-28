struct SceneDecalVolume {
    local_from_world: mat4x4<f32>,
    direction_edge: vec4<f32>,
    surface_angle: vec4<f32>,
    selection: vec4<u32>,
    reserved: vec4<f32>,
}
struct SceneDecalList {
    count: vec4<u32>,
    volumes: array<SceneDecalVolume, 32>,
}
@group(0) @binding(1) var<uniform> scene_decals: SceneDecalList;
@group(0) @binding(2) var scene_decal_albedo: texture_2d_array<f32>;
@group(0) @binding(3) var scene_decal_sampler: sampler;
struct SceneDecalSurface {
    albedo: vec3<f32>,
    properties: vec2<f32>,
}
fn scene_project_decals(position: vec3<f32>, geometric: vec3<f32>, receiver: u32,
    albedo: vec3<f32>, properties: vec2<f32>) -> SceneDecalSurface {
    var result = SceneDecalSurface(albedo, properties);
    let world_dx = dpdx(position);
    let world_dy = dpdy(position);
    for (var i = 0u; i < scene_decals.count.x; i++) {
        let decal = scene_decals.volumes[i];
        if (decal.selection.x & receiver) == 0u { continue; }
        let local = (decal.local_from_world * vec4<f32>(position, 1.0)).xyz;
        let edge = 0.5 - max(max(abs(local.x), abs(local.y)), abs(local.z));
        if edge < 0.0 { continue; }
        let cosine = dot(geometric, decal.direction_edge.xyz);
        if cosine <= decal.surface_angle.z { continue; }
        var weight = 1.0;
        if decal.direction_edge.w > 0.0 { weight *= clamp(edge / decal.direction_edge.w, 0.0, 1.0); }
        if decal.surface_angle.w > 0.0 {
            weight *= clamp((cosine - decal.surface_angle.z) / decal.surface_angle.w, 0.0, 1.0);
        }
        let uv = scene_decal_uv(local, decal.selection.z);
        let dx = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dx, 0.0)).xyz, decal.selection.z);
        let dy = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dy, 0.0)).xyz, decal.selection.z);
        let sample = textureSampleGrad(scene_decal_albedo, scene_decal_sampler, uv + 0.5, i32(decal.selection.y), dx, dy);
        weight *= sample.a * decal.reserved.x;
        result.albedo = mix(result.albedo, sample.rgb, weight);
        result.properties = mix(result.properties, decal.surface_angle.xy, weight);
    }
    return result;
}

fn scene_decal_uv(local: vec3<f32>, axis: u32) -> vec2<f32> {
    if axis == 0u { return local.yz; }
    if axis == 1u { return local.zx; }
    return local.xy;
}
