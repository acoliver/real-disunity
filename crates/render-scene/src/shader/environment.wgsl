// Decode source channels before scaling tangent-space XY.
fn sample_normal(input: VertexOutput, geometric: vec3<f32>, position: vec3<f32>, tangent: vec4<f32>) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u {
        return geometric;
    }
    if input.atlas_eye.w != 0.0 && (material.impostor_relief.x != 0.0 || scene_packed_atlas(input)) { return scene_relief_normal(input); }
    let sampled = scene_atlas_sample(normal_texture, input).rgb * 2.0 - 1.0;
    var xy = sampled.xy;
    var z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    if (material.geometry_flags.w & 1u) != 0u { z = sampled.z; }
    if (material.geometry_flags.w & 2u) != 0u { xy.y = -xy.y; }
    let scale = bitcast<f32>(material.geometry_flags.z);
    if scale == 0.0 { return geometric; }
    xy *= scale;
    var frame_basis = cotangent_frame(geometric, position, input.texcoord);
    if tangent.w != 0.0 {
        let t = normalize(tangent.xyz - geometric * dot(geometric, tangent.xyz));
        frame_basis = mat3x3<f32>(t, cross(geometric, t) * tangent.w, geometric);
    }
    return normalize(frame_basis * vec3<f32>(xy, z));
}

// Shades one fragment. Both entry points funnel here so that the masked and
// opaque pipelines cannot drift apart in anything except the cutout test.
fn shade(original: VertexOutput, front_facing: bool) -> vec4<f32> {
    var input = original;
    input.world_position = scene_relief_position(original);
    var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input), input.tint);

    // A double-sided card is one sheet of geometry standing in for a leaf with
    // two faces. Its stored normal describes the front, so the back must be
    // flipped or the leaf lights as though the sun were behind it.
    var geometric_normal = normalize(input.world_normal);
    if !front_facing {
        geometric_normal = -geometric_normal;
    }

    let mapped_normal = sample_normal(input, geometric_normal, input.world_position, input.world_tangent);

    var normal = scene_fold_direction(mapped_normal, input.ground_fold, material.contact_surface.zw);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        let rough_metal = scene_atlas_sample(rough_metal_texture, input);
        let surface = scene_surface_channels(input, rough_metal);
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
    var ambient_occlusion = 1.0;
    if (material.texture_flags & FLAG_AMBIENT_OCCLUSION) != 0u {
        ambient_occlusion = scene_atlas_sample(ambient_occlusion_texture, input).r;
    }

    if scene_packed_atlas(input) { ambient_occlusion = scene_atlas_first(ambient_occlusion_texture, input).g; }
    var packed_occlusion = ambient_occlusion;
    if material.scattering_mode.y != 0.0 {
        packed_occlusion = min(ambient_occlusion, input.ambient_occlusion);
        ambient_occlusion = packed_occlusion * material.tint_ao.a;
    } else {
        ambient_occlusion *= input.ambient_occlusion;
    }

    var masked_amount = 0.0;
    if material.scattering_mode.y != 0.0 && (!scene_packed_atlas(input) || metalness > 0.05) {
        if scene_packed_atlas(input) {
            masked_amount = metalness * ambient_occlusion;
            metalness = 0.0;
        } else {
            let response = scene_mask_response(metalness, packed_occlusion, ambient_occlusion,
                (material.texture_flags & FLAG_ROUGH_METAL) != 0u);
            metalness = response.x;
            masked_amount = response.y;
        }
    }

    if scene_packed_atlas(input) && metalness > 0.05 { metalness = 0.0; }

    let projected = scene_project_decals(input.world_position, geometric_normal, 1u,
        albedo_sample.rgb, vec2<f32>(roughness, metalness), normal);
    normal = projected.normal;
    albedo_sample = vec4<f32>(projected.albedo, albedo_sample.a);
    roughness = projected.properties.x;
    metalness = projected.properties.y;
    var colour = surface_lighting(
        input.world_position,
        normal,
        albedo_sample.rgb,
        roughness,
        metalness,
        ambient_occlusion,
    );

    if material.scattering_mode.y != 0.0 && frame.grading.y == 0.0 {
        let toward_sun = rd_toward_light(frame.sun_direction.xyz, false);
        let ray = normalize(input.world_position - scene_ray_origin(input.world_position));
        let visibility = sunlight_visibility(input.world_position, normal);
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
            * sunlight_visibility(input.world_position, -normal);
    }

    if (material.texture_flags & FLAG_SELF_ILLUMINATION) != 0u || scene_packed_atlas(input) {
        // The mask is a single channel, so it carries intensity and not colour;
        // the emitted hue can only come from the surface's own albedo.
        var emission = scene_atlas_sample(self_illumination_texture, input).r;
        if scene_packed_atlas(input) { emission = scene_atlas_first(ambient_occlusion_texture, input).r; }
        colour += albedo_sample.rgb * emission;
    }

    // Emission is summed before exposure so that a glowing surface is scaled by
    // the same camera exposure as the sunlight beside it.
    return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);
}

@fragment
fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput {
    let shaded = shade(input, front_facing);
    scene_apply_coverage(input);
    return SceneSurfaceOutput(vec4<f32>(shaded.rgb, 1.0), scene_relief_depth(input));
}


@fragment
fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput {
    let shaded = shade(input, front_facing);
    // Cutout, not blending: the fragment is either present or absent, so no
    // draw ordering is required and the depth buffer stays authoritative.
    if !scene_alpha_covered(shaded.a, input) {
        discard;
    }
    return SceneSurfaceOutput(vec4<f32>(shaded.rgb, 1.0), scene_relief_depth(input));
}

@fragment
fn fs_unlit(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = scene_atlas_sample(albedo_texture, input) * input.tint;
    return vec4<f32>(sampled.rgb, scene_material_alpha(sampled.a));
}

@fragment
fn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input,front_facing);
    scene_apply_coverage(input);
    return vec4<f32>(shaded.rgb,1.0);
}
@fragment
fn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input,front_facing);
    if !scene_alpha_covered(shaded.a,input) { discard; }
    return vec4<f32>(shaded.rgb,1.0);
}
