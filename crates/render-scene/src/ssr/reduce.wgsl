@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source)) / vec2<i32>(SAMPLE_LANES, 1);
    let target_width = (size.x + 1) / 2;
    let sample = i32(pixel.x) / target_width;
    let base = vec2<i32>(i32(pixel.x) - sample * target_width, i32(pixel.y)) * 2;
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let p = base + vec2<i32>(x,y);
            if all(p < size) { sum += textureLoad(source, p + vec2<i32>(sample * size.x, 0), 0); count += 1.0; }
        }
    }
    return sum/count;
}
