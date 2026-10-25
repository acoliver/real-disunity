struct ScreenCamera {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    settings: vec4<f32>,
    samples: vec4<f32>,
}
@group(0) @binding(0) var<uniform> screen: ScreenCamera;
@vertex fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}
fn reconstruct(pixel: vec2<f32>, size: vec2<u32>, depth: f32) -> vec3<f32> {
    let uv = pixel / vec2<f32>(size);
    let point = screen.inverse_projection * vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), depth, 1.0);
    return point.xyz / point.w;
}
