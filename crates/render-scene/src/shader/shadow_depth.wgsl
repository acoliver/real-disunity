@fragment
fn fs_depth_opaque() {}

@fragment
fn fs_depth_masked(input: VertexOutput) {
    let alpha = textureSample(albedo_texture, material_sampler, input.texcoord).a;
    if alpha < material.alpha_cutoff {
        discard;
    }
}
