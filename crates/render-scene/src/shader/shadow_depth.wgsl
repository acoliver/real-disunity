@fragment
fn fs_depth_opaque(input: VertexOutput) {
    scene_apply_coverage(input);
}

@fragment
fn fs_depth_masked(input: VertexOutput) {
    let alpha = textureSample(albedo_texture, material_sampler, input.texcoord).a;
    if !scene_alpha_covered(alpha, input) {
        discard;
    }
}
