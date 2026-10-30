struct BlendedSurface {
    albedo: vec3<f32>, normal: vec3<f32>, roughness: f32, metalness: f32,
}
@group(1) @binding(8) var<storage, read_write> output: array<u32>;
@group(1) @binding(9) var<storage, read> points: array<vec4<f32>>;

fn finite_surface(value: BlendedSurface) -> bool {
    let limit = 3.402823466e38;
    return all(abs(value.albedo) <= vec3<f32>(limit))
        && all(abs(value.normal) <= vec3<f32>(limit))
        && abs(value.roughness) <= limit && abs(value.metalness) <= limit;
}

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = points[id.x];
    let value = cached_surface(p.xy, vec2<f32>(p.z, 0.0), vec2<f32>(0.0, p.w));
    output[id.x] = pack4x8unorm(vec4<f32>(value.albedo, select(0.0, 1.0, finite_surface(value))));
}
