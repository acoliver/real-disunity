fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>, visible: bool) -> METADATA_TYPE {
    var input = original;
    input.world_position = position;
    let footprint = scene_atlas_footprint(albedo_texture, input);
    let geometric = normalize(input.world_normal) * select(-1.0, 1.0, front);
    let normal_preparation = scene_prepare_normal(input, geometric, input.world_position);
    let surface_footprint = scene_prepare_surface_channels(input);
    let decal_gradients = scene_prepare_decals(input.world_position);
    var albedo = scene_albedo(scene_atlas_sample(albedo_texture, input, footprint), input.tint);
    if !scene_geometry_covered(input) || (masked && !(albedo.a >= scene_alpha_cutoff(input, false))) || !visible { discard; }
    let surface = scene_surface_channels_prepared(input, surface_footprint);
    let mapped_normal = sample_normal_prepared(input, geometric, input.world_tangent, normal_preparation);
    var normal = scene_fold_direction(mapped_normal, input.ground_fold, material.contact_surface.zw);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
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
    let projected = scene_project_decals_prepared(input.world_position, geometric, 1u,
        albedo.rgb, vec2<f32>(roughness, metalness), normal, decal_gradients);
    normal = projected.normal;
    albedo = vec4<f32>(projected.albedo, albedo.a);
    roughness = projected.properties.x;
    metalness = projected.properties.y;
    return METADATA_OUTPUT;
}
fn scene_metadata_direction(direction: vec3<f32>) -> vec3<f32> {
    if material.contact_surface.z > 0.0 { return direction; }
    return normalize(direction);
}
