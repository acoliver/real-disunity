@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var occlusion: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(position.xy);
    let size = vec2<i32>(textureDimensions(depths));
    let centre = textureLoad(depths, pixel, 0).r;
    let axis = vec2<i32>(AXIS_X, AXIS_Y);
    var sum = textureLoad(occlusion, pixel, 0).r;
    var total = 1.0;
    for (var tap = -4; tap <= 4; tap++) {
        if tap == 0 { continue; }
        let p = clamp(pixel + axis * tap * 2, vec2<i32>(0), size - vec2<i32>(1));
        let difference = abs(textureLoad(depths, p, 0).r - centre);
        if difference >= 1.0 { continue; }
        let spatial = f32(tap * tap) / (2.0 * (4.0 / 1.5) * (4.0 / 1.5));
        let weight = exp(-7.0 * difference - spatial);
        sum += weight * textureLoad(occlusion, p, 0).r;
        total += weight;
    }
    return sum / total;
}
