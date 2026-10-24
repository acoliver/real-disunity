override MARKER_GREEN: f32 = 1.0;
@group(0) @binding(0) var scene: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if position.y < 4.0 {
        if position.x < 4.0 {
            return vec4<f32>(1.0, MARKER_GREEN, 0.0, 1.0);
        }
        if position.x > f32(textureDimensions(scene).x) - 4.0 {
            return vec4<f32>(0.0, 1.0, 0.0, 1.0);
        }
    }
    return textureLoad(scene, vec2<i32>(position.xy), 0);
}
