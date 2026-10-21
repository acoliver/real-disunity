struct DirectionalShadow {
    clip_from_world: mat4x4<f32>,
    // x: normalized depth bias, y: world-space normal bias in metres.
    bias: vec4<f32>,
}
@group(2) @binding(0) var<uniform> shadow: DirectionalShadow;
@group(2) @binding(1) var shadow_depth: texture_depth_2d;
@group(2) @binding(2) var shadow_sampler: sampler_comparison;

fn sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let clip = shadow.clip_from_world * vec4<f32>(position + normal * shadow.bias.y, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    // Correct each PCF reference for the receiver plane's depth slope. Without
    // this, neighboring taps compare different points on a sloped receiver
    // against the center depth and produce acne even with no caster present.
    let uv_dx = dpdx(uv);
    let uv_dy = dpdy(uv);
    let depth_dx = dpdx(ndc.z);
    let depth_dy = dpdy(ndc.z);
    let depth_gradient = rd_shadow_plane_gradient(uv_dx, uv_dy, depth_dx, depth_dy);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }
    return rd_shadow_2d_grid(shadow_depth, shadow_sampler, uv, ndc.z - shadow.bias.x,
        depth_gradient, i32(shadow.bias.z), true);
}
