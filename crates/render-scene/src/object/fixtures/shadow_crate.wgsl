fn rd_shadow_2d_compare(map: texture_depth_2d, comparison: sampler_comparison,
    uv: vec2<f32>, depth: f32) -> f32 {
    return textureSampleCompareLevel(map, comparison, uv, depth);
}
fn rd_shadow_2d_taps(map: texture_depth_2d, comparison: sampler_comparison,
    uv: vec2<f32>, depth: f32, half_extent_texels: f32) -> vec4<f32> {
    let offset = vec2<f32>(half_extent_texels) / vec2<f32>(textureDimensions(map));
    let a = rd_shadow_2d_compare(map, comparison, uv + vec2<f32>(-offset.x,-offset.y), depth);
    let b = rd_shadow_2d_compare(map, comparison, uv + vec2<f32>( offset.x,-offset.y), depth);
    let c = rd_shadow_2d_compare(map, comparison, uv + vec2<f32>(-offset.x, offset.y), depth);
    let d = rd_shadow_2d_compare(map, comparison, uv + vec2<f32>( offset.x, offset.y), depth);
    return vec4<f32>(a,b,c,d);
}
fn rd_shadow_2d_grid(map: texture_depth_2d, comparison: sampler_comparison,
    uv: vec2<f32>, depth: f32, gradient: vec2<f32>, radius: i32,
    outside_lit: bool) -> f32 {
    let texel = 1.0 / vec2<f32>(textureDimensions(map));
    var sum = 0.0;
    for (var y = -radius; y <= radius; y += 1) {
        for (var x = -radius; x <= radius; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            let tap_uv = uv + offset;
            var tap = 1.0;
            if !outside_lit || (all(tap_uv >= vec2<f32>(0.0)) && all(tap_uv <= vec2<f32>(1.0))) {
                tap = rd_shadow_2d_compare(map, comparison, tap_uv, depth + dot(gradient, offset));
            }
            sum += tap;
        }
    }
    return sum / f32((radius * 2 + 1) * (radius * 2 + 1));
}
fn rd_shadow_2d_bilinear(map: texture_depth_2d, uv: vec2<f32>, depth: f32, gradient: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(map));
    let position = uv * size - vec2<f32>(0.5);
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    var result = 0.0;
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            let delta = vec2<f32>(f32(x), f32(y)) - fraction;
            let corrected = depth + dot(gradient, delta / size);
            let coord = clamp(base + vec2<i32>(x, y), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
            let stored = textureLoad(map, coord, 0);
            let weights = select(vec2<f32>(1.0) - fraction, fraction, vec2<bool>(x == 1, y == 1));
            result += select(0.0, 1.0, corrected < stored) * weights.x * weights.y;
        }
    }
    return result;
}
fn rd_shadow_2d_filter(map: texture_depth_2d, comparison: sampler_comparison,
    uv: vec2<f32>, depth: f32, half_extent_texels: f32, center_only: bool) -> f32 {
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return 1.0; }
    if center_only { return rd_shadow_2d_compare(map, comparison, uv, depth); }
    let taps = rd_shadow_2d_taps(map, comparison, uv, depth, half_extent_texels);
    return (taps.x+taps.y+taps.z+taps.w)*0.25;
}
fn rd_shadow_array_compare(map: texture_depth_2d_array, comparison: sampler_comparison,
    uv: vec2<f32>, layer: i32, depth: f32) -> f32 {
    return textureSampleCompareLevel(map, comparison, uv, layer, depth);
}
fn rd_shadow_array_taps(map: texture_depth_2d_array, comparison: sampler_comparison,
    uv: vec2<f32>, layer: i32, depth: f32, half_extent_texels: f32) -> vec4<f32> {
    let offset = vec2<f32>(half_extent_texels) / vec2<f32>(textureDimensions(map));
    let a = rd_shadow_array_compare(map, comparison, uv + vec2<f32>(-offset.x,-offset.y), layer, depth);
    let b = rd_shadow_array_compare(map, comparison, uv + vec2<f32>( offset.x,-offset.y), layer, depth);
    let c = rd_shadow_array_compare(map, comparison, uv + vec2<f32>(-offset.x, offset.y), layer, depth);
    let d = rd_shadow_array_compare(map, comparison, uv + vec2<f32>( offset.x, offset.y), layer, depth);
    return vec4<f32>(a,b,c,d);
}
fn rd_shadow_array_grid(map: texture_depth_2d_array, comparison: sampler_comparison,
    uv: vec2<f32>, layer: i32, depth: f32, gradient: vec2<f32>, radius: i32,
    outside_lit: bool) -> f32 {
    let texel = 1.0 / vec2<f32>(textureDimensions(map));
    var sum = 0.0;
    for (var y = -radius; y <= radius; y += 1) {
        for (var x = -radius; x <= radius; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            let tap_uv = uv + offset;
            var tap = 1.0;
            if !outside_lit || (all(tap_uv >= vec2<f32>(0.0)) && all(tap_uv <= vec2<f32>(1.0))) {
                tap = rd_shadow_array_compare(map, comparison, tap_uv, layer, depth + dot(gradient, offset));
            }
            sum += tap;
        }
    }
    return sum / f32((radius * 2 + 1) * (radius * 2 + 1));
}
fn rd_shadow_array_bilinear(map: texture_depth_2d_array, uv: vec2<f32>, layer: i32, depth: f32, gradient: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(map));
    let position = uv * size - vec2<f32>(0.5);
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    var result = 0.0;
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            let delta = vec2<f32>(f32(x), f32(y)) - fraction;
            let corrected = depth + dot(gradient, delta / size);
            let coord = clamp(base + vec2<i32>(x, y), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
            let stored = textureLoad(map, coord, layer, 0);
            let weights = select(vec2<f32>(1.0) - fraction, fraction, vec2<bool>(x == 1, y == 1));
            result += select(0.0, 1.0, corrected < stored) * weights.x * weights.y;
        }
    }
    return result;
}
fn rd_shadow_array_filter(map: texture_depth_2d_array, comparison: sampler_comparison,
    uv: vec2<f32>, layer: i32, depth: f32, half_extent_texels: f32, center_only: bool) -> f32 {
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return 1.0; }
    if center_only { return rd_shadow_array_compare(map, comparison, uv, layer, depth); }
    let taps = rd_shadow_array_taps(map, comparison, uv, layer, depth, half_extent_texels);
    return (taps.x+taps.y+taps.z+taps.w)*0.25;
}

fn rd_shadow_plane_gradient(uv_dx: vec2<f32>, uv_dy: vec2<f32>, depth_dx: f32, depth_dy: f32) -> vec2<f32> {
    let determinant = uv_dx.x*uv_dy.y-uv_dx.y*uv_dy.x;
    if abs(determinant) <= 1e-10 { return vec2<f32>(0.0); }
    return vec2<f32>(uv_dy.y*depth_dx-uv_dx.y*depth_dy, uv_dx.x*depth_dy-uv_dy.x*depth_dx)/determinant;
}
// Transform displacements before projection instead of subtracting rounded atlas coordinates.
fn rd_shadow_projected_gradient(clip: vec4<f32>, clip_dx: vec4<f32>, clip_dy: vec4<f32>) -> vec2<f32> {
    let ndc = clip.xyz / clip.w;
    let dx = (clip_dx.xyz - ndc * clip_dx.w) / clip.w;
    let dy = (clip_dy.xyz - ndc * clip_dy.w) / clip.w;
    let uv_scale = vec2<f32>(0.5, -0.5);
    return rd_shadow_plane_gradient(dx.xy * uv_scale, dy.xy * uv_scale, dx.z, dy.z);
}
