/// Instantiates the same sampling algorithm for WGSL's non-generic depth texture types.
#[doc(hidden)]
#[macro_export]
macro_rules! shadow_backend_wgsl {
    ($prefix:literal, $texture:literal, $layer_parameter:literal, $layer_argument:literal) => {
        concat!(
            "fn ", $prefix, "_compare(map: ", $texture, r", comparison: sampler_comparison,
    uv: vec2<f32>", $layer_parameter, r", depth: f32) -> f32 {
    return textureSampleCompareLevel(map, comparison, uv", $layer_argument, r", depth);
}
fn ", $prefix, "_taps(map: ", $texture, r", comparison: sampler_comparison,
    uv: vec2<f32>", $layer_parameter, r", depth: f32, half_extent_texels: f32) -> vec4<f32> {
    let offset = vec2<f32>(half_extent_texels) / vec2<f32>(textureDimensions(map));
    let a = ", $prefix, "_compare(map, comparison, uv + vec2<f32>(-offset.x,-offset.y)", $layer_argument, r", depth);
    let b = ", $prefix, "_compare(map, comparison, uv + vec2<f32>( offset.x,-offset.y)", $layer_argument, r", depth);
    let c = ", $prefix, "_compare(map, comparison, uv + vec2<f32>(-offset.x, offset.y)", $layer_argument, r", depth);
    let d = ", $prefix, "_compare(map, comparison, uv + vec2<f32>( offset.x, offset.y)", $layer_argument, r", depth);
    return vec4<f32>(a,b,c,d);
}
fn ", $prefix, "_grid(map: ", $texture, r", comparison: sampler_comparison,
    uv: vec2<f32>", $layer_parameter, r", depth: f32, gradient: vec2<f32>, radius: i32,
    outside_lit: bool) -> f32 {
    let texel = 1.0 / vec2<f32>(textureDimensions(map));
    var sum = 0.0;
    for (var y = -radius; y <= radius; y += 1) {
        for (var x = -radius; x <= radius; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            let tap_uv = uv + offset;
            var tap = 1.0;
            if !outside_lit || (all(tap_uv >= vec2<f32>(0.0)) && all(tap_uv <= vec2<f32>(1.0))) {
                tap = ", $prefix, "_compare(map, comparison, tap_uv", $layer_argument, r", depth + dot(gradient, offset));
            }
            sum += tap;
        }
    }
    return sum / f32((radius * 2 + 1) * (radius * 2 + 1));
}
fn ", $prefix, "_filter(map: ", $texture, r", comparison: sampler_comparison,
    uv: vec2<f32>", $layer_parameter, r", depth: f32, half_extent_texels: f32, center_only: bool) -> f32 {
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return 1.0; }
    if center_only { return ", $prefix, "_compare(map, comparison, uv", $layer_argument, r", depth); }
    let taps = ", $prefix, "_taps(map, comparison, uv", $layer_argument, r", depth, half_extent_texels);
    return (taps.x+taps.y+taps.z+taps.w)*0.25;
}
"
        )
    };
}

/// Standard sampling kernels; caller owns derivatives, coverage, cascade and bias policy.
#[macro_export]
macro_rules! shadow_wgsl {
    () => {
        concat!(
            $crate::shadow_backend_wgsl!("rd_shadow_2d", "texture_depth_2d", "", ""),
            $crate::shadow_backend_wgsl!("rd_shadow_array", "texture_depth_2d_array", ", layer: i32", ", layer"),
            r"
fn rd_shadow_plane_gradient(uv_dx: vec2<f32>, uv_dy: vec2<f32>, depth_dx: f32, depth_dy: f32) -> vec2<f32> {
    let determinant = uv_dx.x*uv_dy.y-uv_dx.y*uv_dy.x;
    if abs(determinant) <= 1e-10 { return vec2<f32>(0.0); }
    return vec2<f32>(uv_dy.y*depth_dx-uv_dx.y*depth_dy, uv_dx.x*depth_dy-uv_dy.x*depth_dx)/determinant;
}
"
        )
    };
}
