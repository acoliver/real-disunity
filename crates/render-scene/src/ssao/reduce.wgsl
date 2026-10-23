@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let size = vec2<i32>(textureDimensions(source));
    let base = vec2<i32>(position.xy) * 2;
    var nearest = 3.402823e38;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let p = base + vec2<i32>(x, y);
            if all(p < size) { nearest = min(nearest, textureLoad(source, p, 0).r); }
        }
    }
    return nearest;
}
