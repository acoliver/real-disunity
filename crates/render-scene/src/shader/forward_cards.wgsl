// Preserve the ordinary helpers' instruction order; cards capture quad inputs before rejection.
fn scene_prepare_card_occlusion(input: VertexOutput) -> AtlasFootprint {
    var footprint: AtlasFootprint;
    if ((material.texture_flags & FLAG_AMBIENT_OCCLUSION) | (material.wind_clock.z & 2u)) == 0u { return footprint; }
    return scene_atlas_footprint(ambient_occlusion_texture, input);
}
fn scene_prepare_card_emission(input: VertexOutput) -> AtlasFootprint {
    var footprint: AtlasFootprint;
    if (material.texture_flags & FLAG_SELF_ILLUMINATION) == 0u { return footprint; }
    return scene_atlas_footprint(self_illumination_texture, input);
}
fn scene_occlusion_sample_prepared(input: VertexOutput, footprint: AtlasFootprint) -> vec2<f32> {
    if ((material.texture_flags & FLAG_AMBIENT_OCCLUSION) | (material.wind_clock.z & 2u)) == 0u {
        return vec2<f32>(1.0, 0.0);
    }
    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if scene_packed_atlas(input) {
                let sampled = scene_atlas_first(ambient_occlusion_texture, footprint);
                return vec2<f32>(sampled[material.packed_atlas_channels[2].x + 1], sampled[material.packed_atlas_channels[3].x + 0]);
            }
        }
    }
    if (material.texture_flags & FLAG_AMBIENT_OCCLUSION) != 0u {
        return vec2<f32>(scene_atlas_sample(ambient_occlusion_texture, input, footprint)[material.scalar_channels[2].x + 0], 0.0);
    }
    return vec2<f32>(1.0, 0.0);
}
fn scene_emission_sample_prepared(input: VertexOutput, footprint: AtlasFootprint) -> f32 {
    if (material.texture_flags & FLAG_SELF_ILLUMINATION) == 0u { return 0.0; }
    if scene_packed_atlas(input) { return 0.0; }
    return scene_atlas_sample(self_illumination_texture, input, footprint)[material.scalar_channels[3].x + 0];
}
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
fn scene_card_finish_shading(input: VertexOutput, surface: SceneShadingSurface, masked_amount: f32, emission: f32, front_shadow: SceneCardShadow, back_shadow: SceneCardShadow) -> vec4<f32> {
    var normal = surface.normal;
    var albedo_sample = surface.albedo;
    var roughness = surface.properties.x;
    var metalness = surface.properties.y;
    let ambient_occlusion = surface.properties.z;
    let raw = scene_card_shadow_visibility(input.world_position, front_shadow);
    let lighting = scene_surface_lighting_with_visibility(
        input.world_position,
        normal,
        albedo_sample.rgb,
        vec3<f32>(roughness, metalness, ambient_occlusion), DIELECTRIC_F0,
        SceneSurfaceVisibility(raw, raw, direct_sunlight_visibility(raw), 1.0),
    );

    var colour = lighting.color;
    if material.scattering_mode.y != 0.0 && frame.grading.y == 0.0 {
        let toward_sun = rd_toward_light(frame.sun_direction.xyz, false);
        let ray = normalize(input.world_position - scene_ray_origin(input.world_position));
        let visibility = direct_sunlight_visibility(raw);
        let intensity = scene_backlight_strength(ray, toward_sun, normal,
            material.scattering_shape, vec3<f32>(material.scattering_colour.x, masked_amount, visibility));
        colour += scene_scattering_tint(albedo_sample.rgb, material.scattering_colour,
            material.scattering_mode.x) * frame.sun_irradiance.rgb * intensity;
    }
    let translucency = bitcast<f32>(material.geometry_flags.y);
    if translucency != 0.0 && frame.grading.y == 0.0 {
        let light = rd_toward_light(frame.sun_direction.xyz, false);
        colour += albedo_sample.rgb * frame.sun_irradiance.rgb
            * scene_transmission(translucency, -dot(normal, light)) / PI
            * direct_sunlight_visibility(scene_card_shadow_visibility(input.world_position, back_shadow));
    }

    if (material.texture_flags & FLAG_SELF_ILLUMINATION) != 0u || scene_packed_atlas(input) {
        // The mask is a single channel, so it carries intensity and not colour;
        // the emitted hue can only come from the surface's own albedo.
        colour += albedo_sample.rgb * emission;
    }

    // Emission is summed before exposure so that a glowing surface is scaled by
    // the same camera exposure as the sunlight beside it.
    return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);
}
