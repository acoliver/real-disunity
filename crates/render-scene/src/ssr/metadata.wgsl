fn reflection_metadata(input: VertexOutput, front: bool, masked: bool) -> vec4<f32> {
    let albedo = textureSample(albedo_texture, material_sampler, input.texcoord);
    let geometric = normalize(input.world_normal) * select(-1.0, 1.0, front);
    let normal = sample_normal(input.texcoord, geometric, input.world_position);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        let rm = textureSample(rough_metal_texture, material_sampler, input.texcoord);
        roughness *= rm.g;
        metalness *= rm.b;
    }
    if masked && !scene_alpha_covered(albedo.a) { discard; }
    return METADATA_OUTPUT;
}
@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return reflection_metadata(input, front, false);
}
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return reflection_metadata(input, front, true);
}
