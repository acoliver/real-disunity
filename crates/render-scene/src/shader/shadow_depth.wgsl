@vertex
fn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    return material_vertex_pass(vertex, instance, false);
}

@fragment fn fs_depth_opaque(input: VertexOutput) { scene_apply_coverage(input); }
@fragment fn fs_depth_masked(input: VertexOutput) { if !scene_alpha_covered_pass(scene_material_alpha(scene_atlas_sample(albedo_texture,input).a),input,true) { discard; } }
