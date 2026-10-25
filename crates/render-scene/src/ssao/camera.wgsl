struct ScreenCamera {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    settings: vec4<f32>,
    samples: vec4<f32>,
}
@group(3) @binding(0) var<uniform> screen: ScreenCamera;
