struct LayeredPositions {
    footprint: vec2<f32>,
    points: array<vec2<f32>>,
}
@group(0) @binding(3) var<storage, read> layered_positions: LayeredPositions;
@group(0) @binding(4) var<storage, read_write> layered_products: array<vec4<u32>>;

@compute @workgroup_size(64)
fn bake_layered(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&layered_positions.points) { return; }
    let result = compose_layered(layered_positions.points[id.x],
        vec2<f32>(layered_positions.footprint.x, 0.0), vec2<f32>(0.0, layered_positions.footprint.y));
    layered_products[id.x] = vec4<u32>(pack4x8unorm(vec4<f32>(result.albedo, result.metallic)),
        pack4x8unorm(vec4<f32>(result.normal * 0.5 + 0.5, result.roughness)),
        u32(round(clamp(result.height, 0.0, 1.0) * 255.0)), 0u);
}
