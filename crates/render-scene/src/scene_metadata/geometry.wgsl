fn reflection_metadata(original: VertexOutput, front: bool, masked: bool) -> vec4<f32> {
    var input = original;
    input.world_position = scene_relief_position(original);
    var albedo = scene_albedo(scene_atlas_sample(albedo_texture, input), input.tint);
    let geometric = normalize(input.world_normal) * select(-1.0, 1.0, front);
    let mapped_normal = sample_normal(input, geometric, input.world_position, input.world_tangent);
    let normal = scene_fold_direction(mapped_normal, input.ground_fold, material.contact_surface.zw);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        let rm = scene_atlas_sample(rough_metal_texture, input);
        let surface = scene_surface_channels(input, rm);
        roughness = surface.x;
        metalness = surface.y;
    }
    let contact_weight = scene_contact(input.world_position);
    if contact_weight != 0.0 {
        albedo = vec4<f32>(scene_contact_albedo(albedo.rgb, contact_weight), albedo.a);
        let surface = scene_contact_surface(vec2<f32>(roughness, metalness), contact_weight);
        roughness = surface.x;
        metalness = surface.y;
    }
    if material.scattering_mode.y != 0.0 && (!scene_packed_atlas(input) || metalness > 0.05) { metalness = 0.0; }
    if scene_packed_atlas(input) && metalness > 0.05 { metalness = 0.0; }
    scene_apply_coverage(input);
    if masked && !scene_alpha_covered(albedo.a, input) { discard; }
    return METADATA_OUTPUT;
}
@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    return SceneSurfaceOutput(reflection_metadata(input, front, false),scene_relief_depth(input));
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    return SceneSurfaceOutput(reflection_metadata(input, front, true),scene_relief_depth(input));
}

fn scene_metadata_direction(direction: vec3<f32>) -> vec3<f32> {
    if material.contact_surface.z > 0.0 { return direction; }
    return normalize(direction);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,false); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,true); }
