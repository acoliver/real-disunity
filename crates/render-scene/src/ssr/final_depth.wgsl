@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) f32 {
    return textureLoad(source, vec2<i32>(pixel.xy), 0).r;
}
