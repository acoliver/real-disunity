// Pair the six phase-interpolated coefficients into three linear comparisons.
fn rd_shadow_pcf5_weights(phase: f32) -> vec3<f32> {
    return vec3<f32>(4.0 - 3.0 * phase, 7.0, 1.0 + 3.0 * phase);
}
fn rd_shadow_pcf5_offsets(phase: f32, weights: vec3<f32>) -> vec3<f32> {
    return vec3<f32>((3.0 - 2.0 * phase) / weights.x - 2.0,
        (3.0 + phase) / 7.0, phase / weights.z + 2.0);
}
fn rd_shadow_array_weighted_pcf5(map: texture_depth_2d_array, comparison: sampler_comparison,
    uv: vec2<f32>, layer: i32, depth: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(map));
    let anchor = floor(uv * size + vec2<f32>(0.5)) - vec2<f32>(0.5);
    let phase = uv * size - anchor;
    let wx = rd_shadow_pcf5_weights(phase.x);
    let wy = rd_shadow_pcf5_weights(phase.y);
    let ox = rd_shadow_pcf5_offsets(phase.x, wx);
    let oy = rd_shadow_pcf5_offsets(phase.y, wy);
    var sum = 0.0;
    for (var y = 0; y < 3; y += 1) {
        for (var x = 0; x < 3; x += 1) {
            let tap = (anchor + vec2<f32>(ox[x], oy[y])) / size;
            sum += wx[x] * wy[y] * textureSampleCompareLevel(map, comparison, tap, layer, depth);
        }
    }
    return sum / 144.0;
}
