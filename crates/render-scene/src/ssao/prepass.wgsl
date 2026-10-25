fn prepass_normal(input: VertexOutput, front: bool) -> vec4<f32> {
    let normal = normalize(input.world_normal) * select(-1.0, 1.0, front);
    return vec4<f32>(normalize((screen.view * vec4<f32>(normal, 0.0)).xyz), 1.0);
}
@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    return prepass_normal(input, front);
}
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let alpha = textureSample(albedo_texture, material_sampler, input.texcoord).a;
    if alpha < material.alpha_cutoff { discard; }
    return prepass_normal(input, front);
}
