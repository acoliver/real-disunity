@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let base = vec2<i32>(position.xy) * 2;
    let last = vec2<i32>(textureDimensions(source)) - 1;
    let a = textureLoad(source, min(base, last), 0).r;
    let b = textureLoad(source, min(base + vec2<i32>(1, 0), last), 0).r;
    let c = textureLoad(source, min(base + vec2<i32>(0, 1), last), 0).r;
    let d = textureLoad(source, min(base + vec2<i32>(1), last), 0).r;
    return (a + b + c + d) * 0.25;
}
