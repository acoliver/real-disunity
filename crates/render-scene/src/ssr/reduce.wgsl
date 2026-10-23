@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let base = vec2<i32>(pixel.xy) * 2;
    let size = vec2<i32>(textureDimensions(source));
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let p = base + vec2<i32>(x,y);
            if all(p < size) { sum += textureLoad(source, p, 0); count += 1.0; }
        }
    }
    return sum/count;
}
