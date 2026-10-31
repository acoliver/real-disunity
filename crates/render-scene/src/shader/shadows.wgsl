fn sunlight_normal_bias() -> f32 { return shadow.bias.y; }
struct DirectionalShadow {
    clip_from_world: array<mat4x4<f32>, 4>,
    // y: normal bias in metres, z: grid radius or -1 for bilinear, w: strength reduction.
    bias: vec4<f32>,
    // count, blend fraction, camera split policy, camera near distance.
    policy: vec4<f32>,
    splits: vec4<f32>,
    view_depth: vec4<f32>,
    depth_bias: vec4<f32>,
}
@group(2) @binding(0) var<uniform> shadow: DirectionalShadow;
@group(2) @binding(1) var shadow_depth: texture_depth_2d_array;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;

fn cascade_visibility(ndc: vec3<f32>, gradient: vec2<f32>, layer: i32) -> f32 {
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }
    if shadow.bias.z < 0.0 {
        return rd_shadow_array_bilinear(shadow_depth, uv, layer, ndc.z - shadow.depth_bias[layer], gradient);
    }
    return rd_shadow_array_grid(shadow_depth, shadow_sampler, uv, layer, ndc.z - shadow.depth_bias[layer],
        gradient, i32(shadow.bias.z), true);
}

fn single_shadow_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    // Keep the single-map projection scalar through sampling to preserve its rounding.
    let clip = shadow.clip_from_world[0] * vec4<f32>(position + normal * shadow.bias.y, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let gradient = rd_shadow_plane_gradient(dpdx(uv), dpdy(uv), dpdx(ndc.z), dpdy(ndc.z));
    return cascade_visibility(ndc, gradient, 0);
}

fn raw_sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    // The layer count is draw-uniform, so both derivative paths remain uniform.
    if shadow.policy.x == 1.0 && shadow.policy.z < 0.5 { return single_shadow_visibility(position, normal); }
    var coordinates: array<vec3<f32>, 4>;
    var gradients: array<vec2<f32>, 4>;
    // Derivatives must precede divergent coverage and cascade selection.
    for (var i = 0; i < 4; i += 1) {
        let clip = shadow.clip_from_world[i] * vec4<f32>(position + normal * shadow.bias.y, 1.0);
        let ndc = clip.xyz / clip.w;
        let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
        coordinates[i] = ndc;
        gradients[i] = rd_shadow_plane_gradient(dpdx(uv), dpdy(uv), dpdx(ndc.z), dpdy(ndc.z));
    }
    let count = i32(shadow.policy.x);
    let distance = dot(shadow.view_depth, vec4<f32>(position, 1.0));
    var selected = count;
    for (var i = 0; i < count; i += 1) {
        var contains = max(abs(coordinates[i].x), abs(coordinates[i].y)) <= 1.0;
        if shadow.policy.z > 0.5 { contains = distance <= shadow.splits[i]; }
        if contains { selected = i; break; }
    }
    if selected == count { return 1.0; }
    let current = cascade_visibility(coordinates[selected], gradients[selected], selected);
    if selected + 1 == count || shadow.policy.y == 0.0 { return current; }
    var progress = max(abs(coordinates[selected].x), abs(coordinates[selected].y));
    if shadow.policy.z > 0.5 {
        var start = shadow.policy.w;
        if selected > 0 { start = shadow.splits[selected - 1]; }
        progress = (distance - start) / (shadow.splits[selected] - start);
    }
    var fade_fraction = shadow.policy.y;
    if shadow.policy.z > 1.5 {
        var width = shadow.splits[0] - shadow.policy.w;
        var start = shadow.policy.w;
        var fade = width * shadow.policy.y;
        for (var i = 0; i < selected; i += 1) {
            start += width - fade;
            width *= shadow.bias.x;
            fade = (width - fade) * shadow.policy.y;
        }
        progress = (distance - start) / width;
        fade_fraction = fade / width;
    }
    let weight = clamp((progress - (1.0 - fade_fraction)) / fade_fraction, 0.0, 1.0);
    let next = cascade_visibility(coordinates[selected + 1], gradients[selected + 1], selected + 1);
    return mix(current, next, weight);
}

fn direct_sunlight_visibility(visibility: f32) -> f32 {
    if shadow.bias.w == 0.0 { return visibility; }
    return mix(1.0, visibility, 1.0 - shadow.bias.w);
}

fn sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    return direct_sunlight_visibility(raw_sunlight_visibility(position, normal));
}
