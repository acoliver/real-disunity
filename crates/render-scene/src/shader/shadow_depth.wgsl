@vertex
fn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    return material_vertex_pass(vertex, instance, false);
}

@fragment fn fs_depth_opaque(input: VertexOutput) { scene_apply_coverage(input); }
@fragment fn fs_depth_masked(input: VertexOutput) { if !scene_alpha_covered_pass(scene_sampled_alpha(input),input,true) { discard; } }
