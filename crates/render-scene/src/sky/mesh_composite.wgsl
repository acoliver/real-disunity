@group(0) @binding(0) var radiance:texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
    let xy=vec2(f32((index<<1u)&2u),f32(index&2u))*2.0-1.0;
    return vec4(xy,1.0,1.0);
}
@fragment fn fragment(@builtin(position) pixel:vec4<f32>,@builtin(sample_index) sample:u32)->@location(0) vec4<f32> {
    return textureLoad(radiance,vec2<i32>(pixel.xy),i32(sample));
}
