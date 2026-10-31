fn receiver_gather(uv: vec2<f32>, receiver_depth: f32) -> f32 {
    let size = vec2<i32>(textureDimensions(ambient_texture));
    let base = vec2<i32>(floor(uv * vec2<f32>(size) - vec2<f32>(0.5)));
    let a = textureLoad(ambient_texture, clamp(base, vec2<i32>(0), size - 1), 0).rg;
    let b = textureLoad(ambient_texture, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), size - 1), 0).rg;
    let c = textureLoad(ambient_texture, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), size - 1), 0).rg;
    let e = textureLoad(ambient_texture, clamp(base + vec2<i32>(1), vec2<i32>(0), size - 1), 0).rg;
    let depths = vec4<f32>(a.y, b.y, c.y, e.y);
    let covered = depths > vec4<f32>(0.0);
    if !any(covered) { return 1.0; }
    let high = max(max(a.y, b.y), max(c.y, e.y));
    let valid_depths = select(vec4<f32>(high), depths, covered);
    let low = min(min(valid_depths.x, valid_depths.y), min(valid_depths.z, valid_depths.w));
    let depth = clamp(receiver_depth, low, high);
    let distance = select(vec4<f32>(high - low), abs(vec4<f32>(depth) - depths), covered);
    // A shared exponent offset cancels in the quotient and avoids underflow.
    let weights = select(vec4<f32>(0.0), exp(vec4<f32>(min(min(distance.x, distance.y), min(distance.z, distance.w))) - distance), covered);
    return dot(weights, vec4<f32>(a.x, b.x, c.x, e.x)) / dot(weights, vec4<f32>(1.0));
}
