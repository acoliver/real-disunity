struct Sample {
    color: u32,
    normal_x: i32,
    normal_y: i32,
    normal_z: i32,
    roughness: u32,
    weight: u32,
}
@group(0) @binding(0) var<storage, read> ranges: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read> samples: array<Sample>;
@group(0) @binding(2) var<storage, read_write> products: array<vec2<u32>>;

fn unpack_color(value: u32) -> vec4<u32> {
    return vec4<u32>(value & 255u, (value >> 8u) & 255u, (value >> 16u) & 255u, value >> 24u);
}
fn pack_bytes(value: vec4<u32>) -> u32 {
    return value.x | (value.y << 8u) | (value.z << 16u) | (value.w << 24u);
}
@compute @workgroup_size(64)
fn compose(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&ranges) { return; }
    let span = ranges[id.x];
    var color = vec4<u32>(0u);
    var roughness = 0u;
    var normal = vec3<f32>(0.0);
    var total = 0u;
    for (var index = span.x; index < span.x + span.y; index += 1u) {
        let sample = samples[index];
        if sample.weight == 0u { continue; }
        color += unpack_color(sample.color) * sample.weight;
        roughness += sample.roughness * sample.weight;
        normal += normalize(vec3<f32>(f32(sample.normal_x), f32(sample.normal_y), f32(sample.normal_z))) * f32(sample.weight);
        total += sample.weight;
    }
    let direction = normalize(normal);
    let encoded_normal = vec3<u32>(floor(clamp((direction * 0.5 + 0.5) * 255.0, vec3<f32>(0.0), vec3<f32>(255.0)) + 0.5));
    products[id.x] = vec2<u32>(pack_bytes((color + vec4<u32>(total / 2u)) / total),
        pack_bytes(vec4<u32>(encoded_normal, (roughness + total / 2u) / total)));
}
