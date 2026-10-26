@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var input_ao: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) vec2<f32> {
    let pixel = vec2<i32>(position.xy);
    let size = vec2<i32>(textureDimensions(depths));
    let centre = textureLoad(input_ao, pixel, 0).rg;
    let far_point = reconstruct(vec2<f32>(0.5), vec2<u32>(1), 1.0);
    var sum = centre.x;
    var total = 1.0;
    for (var offset = -TAP_RADIUS; offset <= TAP_RADIUS; offset++) {
        if offset == 0 { continue; }
        let coordinate = min(pixel + vec2<i32>(AXIS_X, AXIS_Y) * offset * TAP_STEP, size);
        var sample = vec2<f32>(0.0);
        if all(coordinate >= vec2<i32>(0)) && all(coordinate < size) {
            sample = textureLoad(input_ao, coordinate, 0).rg;
        }
        let exponent = abs(centre.y - sample.y) * DEPTH_COEFFICIENT * -far_point.z;
        let weight = exp(-exponent - f32(offset * offset) / VARIANCE);
        sum += sample.x * weight;
        total += weight;
    }
    return vec2<f32>(min(1.01 * sum / total, 1.0), centre.y);
}
