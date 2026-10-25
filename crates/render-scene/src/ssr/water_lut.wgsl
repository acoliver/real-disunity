fn water_coefficients(uv: vec2<f32>) -> vec2<f32> {
    let size = vec2<i32>(textureDimensions(water_lut));
    let position = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * vec2<f32>(size) - 0.5;
    let lower = vec2<i32>(floor(position));
    let a = clamp(lower, vec2<i32>(0), size - 1);
    let b = clamp(lower + 1, vec2<i32>(0), size - 1);
    let weight = fract(position);
    return mix(mix(textureLoad(water_lut, a, 0).rg,
        textureLoad(water_lut, vec2<i32>(b.x, a.y), 0).rg, weight.x),
        mix(textureLoad(water_lut, vec2<i32>(a.x, b.y), 0).rg,
        textureLoad(water_lut, b, 0).rg, weight.x), weight.y);
}
