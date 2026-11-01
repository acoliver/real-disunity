// Preparation of the temporal mask at the internal extent R: every texel becomes the maximum, per
// channel, of the 3x3 texels around it (texels outside the image do not count). Channel x is the
// reactive strength and y the composition strength, see the producer contract in
// docs/rendering.md. Values pass through unchanged where the neighbourhood is uniform.
@group(0) @binding(0) var mask: texture_2d<f32>;

@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn dilate(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let centre = vec2<i32>(floor(position.xy));
    let last = vec2<i32>(textureDimensions(mask)) - 1;
    var best = vec2<f32>(0.0);
    for (var i = 0; i < 9; i++) {
        let at = clamp(centre + vec2<i32>(i % 3 - 1, i / 3 - 1), vec2<i32>(0), last);
        best = max(best, textureLoad(mask, at, 0).xy);
    }
    return vec4<f32>(best, 0.0, 0.0);
}
