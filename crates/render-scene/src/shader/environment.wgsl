// Reconstructs a full normal from the two channels BC5 preserves.
fn sample_normal(uv: vec2<f32>, geometric: vec3<f32>, position: vec3<f32>) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u {
        return geometric;
    }
    let xy = textureSample(normal_texture, material_sampler, uv).rg * 2.0 - 1.0;
    let z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    let frame_basis = cotangent_frame(geometric, position, uv);
    return normalize(frame_basis * vec3<f32>(xy, z));
}

// Shades one fragment. Both entry points funnel here so that the masked and
// opaque pipelines cannot drift apart in anything except the cutout test.
fn shade(input: VertexOutput, front_facing: bool) -> vec4<f32> {
    var albedo_sample = scene_albedo(textureSample(albedo_texture, material_sampler, input.texcoord), input.tint);

    // A double-sided card is one sheet of geometry standing in for a leaf with
    // two faces. Its stored normal describes the front, so the back must be
    // flipped or the leaf lights as though the sun were behind it.
    var geometric_normal = normalize(input.world_normal);
    if !front_facing {
        geometric_normal = -geometric_normal;
    }

    let normal = sample_normal(input.texcoord, geometric_normal, input.world_position);

    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        let rough_metal = textureSample(rough_metal_texture, material_sampler, input.texcoord);
        roughness *= rough_metal.g;
        metalness *= rough_metal.b;
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
        ambient_occlusion = textureSample(
            ambient_occlusion_texture,
            material_sampler,
            input.texcoord,
        ).r;
    }

    ambient_occlusion *= input.ambient_occlusion;

    var colour = surface_lighting(
        input.world_position,
        normal,
        albedo_sample.rgb,
        roughness,
        metalness,
        ambient_occlusion,
    );

    let translucency = bitcast<f32>(material.geometry_flags.y);
    if translucency != 0.0 && frame.grading.y == 0.0 {
        let light = rd_toward_light(frame.sun_direction.xyz, false);
        colour += albedo_sample.rgb * frame.sun_irradiance.rgb
            * scene_transmission(translucency, -dot(normal, light)) / PI
            * sunlight_visibility(input.world_position, -normal);
    }

    if (material.texture_flags & FLAG_SELF_ILLUMINATION) != 0u {
        // The mask is a single channel, so it carries intensity and not colour;
        // the emitted hue can only come from the surface's own albedo.
        let emission = textureSample(
            self_illumination_texture,
            material_sampler,
            input.texcoord,
        ).r;
        colour += albedo_sample.rgb * emission;
    }

    // Emission is summed before exposure so that a glowing surface is scaled by
    // the same camera exposure as the sunlight beside it.
    return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);
}

@fragment
fn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input, front_facing);
    scene_apply_coverage(input);
    return vec4<f32>(shaded.rgb, 1.0);
}


@fragment
fn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input, front_facing);
    // Cutout, not blending: the fragment is either present or absent, so no
    // draw ordering is required and the depth buffer stays authoritative.
    if !scene_alpha_covered(shaded.a, input) {
        discard;
    }
    return vec4<f32>(shaded.rgb, 1.0);
}

@fragment
fn fs_unlit(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(albedo_texture, material_sampler, input.texcoord) * input.tint;
}
