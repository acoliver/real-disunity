@group(0) @binding(1) var source: texture_depth_2d;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let depth = textureLoad(source, vec2<i32>(position.xy), 0);
    return -reconstruct(position.xy, textureDimensions(source), depth).z;
}
