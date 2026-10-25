@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var normals: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let normal = textureLoad(normals, pixel, 0);
    if normal.a == 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(normal.xyz, textureLoad(depths, pixel, 0).r);
}
