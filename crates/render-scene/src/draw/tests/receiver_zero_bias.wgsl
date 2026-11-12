@group(0) @binding(0) var<storage, read> operands: array<vec4<u32>>;
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.5, 1.0);
}
struct Output {
    @location(0) a: vec4<f32>,
    @location(1) b: vec2<f32>,
}
@fragment fn fragment(@builtin(position) pixel: vec4<f32>) -> Output {
    let index = u32(pixel.x) + u32(pixel.y) * 4u;
    let operand = bitcast<vec4<f32>>(operands[index]);
    let position = vec3<f32>(pixel.xy, operand.w);
    scene_capture_receiver(position, operand.xyz);
    if (index & 1u) != 0u { discard; }
    return Output(receiver_0_a, receiver_0_b);
}
