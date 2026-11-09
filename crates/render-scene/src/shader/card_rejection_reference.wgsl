fn scene_card_shade(original: VertexOutput, front_facing: bool, position: vec3<f32>, masked: bool, visible: bool) -> vec4<f32> {
    var input = original;
    input.world_position = position;
    let albedo_footprint = scene_atlas_footprint(albedo_texture, input);
    var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input, albedo_footprint), input.tint);
    let surface = scene_surface_channels(input);
    let coverage_alpha = scene_mask_alpha(albedo_sample.a, input);
    let occlusion_footprint = scene_prepare_card_occlusion(input);
    let emission_footprint = scene_prepare_card_emission(input);

    // A double-sided card is one sheet of geometry standing in for a leaf with
    // two faces. Its stored normal describes the front, so the back must be
    // flipped or the leaf lights as though the sun were behind it.
    let geometric_normal = normalize(input.world_normal) * select(-1.0, 1.0, front_facing);

    let mapped_normal = sample_normal(input, geometric_normal, input.world_position, input.world_tangent);

    var normal = scene_fold_direction(mapped_normal, input.ground_fold, material.contact_surface.zw);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        roughness = surface.x;
        metalness = surface.y;
    }
    let contact_weight = scene_contact(input.world_position);
    if contact_weight != 0.0 {
        albedo_sample = vec4<f32>(scene_contact_albedo(albedo_sample.rgb, contact_weight), albedo_sample.a);
        let surface = scene_contact_surface(vec2<f32>(roughness, metalness), contact_weight);
        roughness = surface.x;
        metalness = surface.y;
    }
    // Scattering's metalness decision is independent of occlusion; its amount is deferred.
    let decal_metalness = scene_shading_scattering(input, metalness, vec2<f32>(1.0)).x;
    let projected = scene_project_decals(input.world_position, geometric_normal, 1u,
        albedo_sample.rgb, vec2<f32>(roughness, decal_metalness), normal);
    let front_shadow = scene_prepare_card_shadow(input.world_position, projected.normal);
    var back_shadow: SceneCardShadow;
    if bitcast<f32>(material.geometry_flags.y) != 0.0 && frame.grading.y == 0.0 {
        back_shadow = scene_prepare_card_shadow(input.world_position, -projected.normal);
    }
    if !visible || !scene_geometry_covered(input) { discard; }
    if masked {
        if SCENE_ALPHA_TO_COVERAGE {
            if coverage_alpha == 0.0 { discard; }
        } else {
            if !scene_alpha_covered(albedo_sample.a, input) { discard; }
        }
    }
    let sampled_occlusion = scene_occlusion_sample_prepared(input, occlusion_footprint);
    let sampled_emission = scene_emission_sample_prepared(input, emission_footprint);
    let auxiliary = vec2<f32>(sampled_occlusion.x, select(sampled_emission, sampled_occlusion.y, scene_packed_atlas(input)));
    let occlusion = scene_shading_occlusion(input, auxiliary.x);
    let scattering = scene_shading_scattering(input, metalness, occlusion);
    let surface_material = SceneShadingSurface(vec4<f32>(projected.albedo, albedo_sample.a), projected.normal,
        vec3<f32>(projected.properties, occlusion.x));
    let shaded = scene_card_finish_shading(input, surface_material, scattering.y, auxiliary.y, front_shadow, back_shadow);
    return vec4<f32>(shaded.rgb, select(1.0, coverage_alpha, masked));
}
