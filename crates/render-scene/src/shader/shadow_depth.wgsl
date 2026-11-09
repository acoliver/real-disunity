@vertex
fn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    return material_vertex_pass(vertex, instance, false);
}

@fragment fn fs_depth_opaque(input: VertexOutput) { scene_apply_coverage(input); }
@fragment fn fs_depth_masked(input: VertexOutput) {
    if SCENE_IMPOSTOR {
        let footprint = scene_atlas_footprint(albedo_texture, input);
        if !scene_geometry_covered(input) { discard; }
        let alpha = scene_material_alpha(scene_atlas_sample(albedo_texture, input, footprint).a);
        if !scene_alpha_covered_pass(alpha,input,true) { discard; }
    } else {
        if !scene_alpha_covered_pass(scene_sampled_alpha(input),input,true) { discard; }
    }
}
