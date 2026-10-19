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
    let determinant = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;
    var depth_gradient = vec2<f32>(0.0);
    if abs(determinant) > 1e-10 {
        depth_gradient = vec2<f32>(uv_dy.y * depth_dx - uv_dx.y * depth_dy,
                                   uv_dx.x * depth_dy - uv_dy.x * depth_dx) / determinant;
    }
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_depth));
    var visibility = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            let tap_uv = uv + offset;
            // Taps outside coverage are lit, not clamped to an edge occluder.
            var tap = 1.0;
            if all(tap_uv >= vec2<f32>(0.0)) && all(tap_uv <= vec2<f32>(1.0)) {
                tap = textureSampleCompareLevel(shadow_depth, shadow_sampler, tap_uv, ndc.z + dot(depth_gradient, offset) - shadow.bias.x);
            }
            visibility += tap;
        }
    }
    return visibility / 9.0;
}
