struct BlendedSurface {
    albedo: vec3<f32>, normal: vec3<f32>, roughness: f32, metalness: f32,
}
@group(0) @binding(0) var<storage, read> taps: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;

@compute @workgroup_size(1)
fn main() {
    let first = taps[0].xyz;
    let sum = first + taps[1].xyz;
    output[0] = vec4<f32>(sum, 0.0);
    output[1] = vec4<f32>(cached_normal_direction(sum, first), 0.0);
}
