
fn scene_shading_occlusion(input: VertexOutput, sampled: f32) -> vec2<f32> {
    var ambient_occlusion = sampled;
    var packed_occlusion = ambient_occlusion;
    if material.scattering_mode.y != 0.0 {
        packed_occlusion = min(ambient_occlusion, input.ambient_occlusion);
        ambient_occlusion = packed_occlusion * material.tint_ao.a;
    } else {
        ambient_occlusion *= input.ambient_occlusion;
    }

    return vec2<f32>(ambient_occlusion, packed_occlusion);
}
fn scene_occlusion_sample(input: VertexOutput) -> vec2<f32> {
    if ((material.texture_flags & FLAG_AMBIENT_OCCLUSION) | (material.wind_clock.z & 2u)) == 0u {
        return vec2<f32>(1.0, 0.0);
    }
    let footprint = scene_atlas_footprint(ambient_occlusion_texture, input);
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
fn scene_emission_sample(input: VertexOutput) -> f32 {
    if (material.texture_flags & FLAG_SELF_ILLUMINATION) == 0u { return 0.0; }
    let footprint = scene_atlas_footprint(self_illumination_texture, input);
    if scene_packed_atlas(input) { return 0.0; }
    return scene_atlas_sample(self_illumination_texture, input, footprint)[material.scalar_channels[3].x + 0];
}
fn scene_auxiliary_samples(input: VertexOutput) -> vec2<f32> {
    let occlusion = scene_occlusion_sample(input);
    let emission = scene_emission_sample(input);
    return vec2<f32>(occlusion.x, select(emission, occlusion.y, scene_packed_atlas(input)));
}
fn scene_shading_scattering(input: VertexOutput, original: f32, occlusion: vec2<f32>) -> vec2<f32> {
    var metalness = original;
    var masked_amount = 0.0;
    if material.scattering_mode.y != 0.0 && (!scene_packed_atlas(input) || metalness > 0.05) {
        if scene_packed_atlas(input) {
            masked_amount = metalness * occlusion.x;
            metalness = 0.0;
        } else {
            let response = scene_mask_response(metalness, occlusion.y, occlusion.x,
                (material.texture_flags & FLAG_ROUGH_METAL) != 0u);
            metalness = response.x;
            masked_amount = response.y;
        }
    }

    if scene_packed_atlas(input) && metalness > 0.05 { metalness = 0.0; }

    return vec2<f32>(metalness, masked_amount);
}
struct SceneShadingSurface {
    albedo: vec4<f32>,
    normal: vec3<f32>,
    properties: vec3<f32>,
}

// The constant material values that shading reads besides the surface's own channels: the
// lighting record, read from the material uniform.
struct SceneLightingRecord {
    // Back-scattering distortion, falloff, sun scale and ambient scale.
    scattering_shape: vec4<f32>,
    // Back-scattering scale, luminance, saturation and base temperature in kelvin.
    scattering_colour: vec4<f32>,
    // x: warming in kelvin; y: nonzero enables back-scattering; z and w are not read.
    scattering_mode: vec4<f32>,
    // x: transmission amount; y, z and w are zero.
    transmission: vec4<f32>,
}

fn scene_material_lighting_record() -> SceneLightingRecord {
    return SceneLightingRecord(material.scattering_shape, material.scattering_colour,
        material.scattering_mode, vec4<f32>(bitcast<f32>(material.geometry_flags.y), 0.0, 0.0, 0.0));
}

// Whether a record's back-scattering term applies. Forward draws test their material through
// the argument-free accessors below, which the shadow uniformity check reads as uniform.
fn scene_record_scatters(record: SceneLightingRecord) -> bool {
    return (record.scattering_mode.y != 0.0) & (frame.grading.y == 0.0);
}

fn scene_material_scatters() -> bool {
    return (material.scattering_mode.y != 0.0) & (frame.grading.y == 0.0);
}

// Sun light scattered through a thin surface toward the viewer. `visibility` is the sun
// visibility of the lit side.
fn scene_record_scattering(position: vec3<f32>, normal: vec3<f32>, albedo: vec3<f32>,
    record: SceneLightingRecord, masked_amount: f32, visibility: f32) -> vec3<f32> {
    let toward_sun = rd_toward_light(frame.sun_direction.xyz, false);
    let ray = normalize(position - scene_ray_origin(position));
    let intensity = scene_backlight_strength(ray, toward_sun, normal,
        record.scattering_shape, vec3<f32>(record.scattering_colour.x, masked_amount, visibility));
    return scene_scattering_tint(albedo, record.scattering_colour,
        record.scattering_mode.x) * frame.sun_irradiance.rgb * intensity;
}

fn scene_record_transmits(record: SceneLightingRecord) -> bool {
    return (record.transmission.x != 0.0) & (frame.grading.y == 0.0);
}

fn scene_material_transmits() -> bool {
    return (bitcast<f32>(material.geometry_flags.y) != 0.0) & (frame.grading.y == 0.0);
}

// Sun light transmitted through the surface from behind. `visibility` is the sun
// visibility of the far side.
fn scene_record_transmission(normal: vec3<f32>, albedo: vec3<f32>, record: SceneLightingRecord,
    visibility: f32) -> vec3<f32> {
    let light = rd_toward_light(frame.sun_direction.xyz, false);
    return albedo * frame.sun_irradiance.rgb
        * scene_transmission(record.transmission.x, -dot(normal, light)) / PI
        * visibility;
}

// An object fragment's material after projected decals: the values the forward path lights.
// surface.properties holds roughness, metalness and AO; masked_amount is the per-pixel
// scattering amount.
struct SceneObjectMaterial {
    geometric_normal: vec3<f32>,
    surface: SceneShadingSurface,
    masked_amount: f32,
    emission: f32,
}

// Shades one fragment. Both entry points funnel here so that the masked and
// opaque pipelines cannot drift apart in anything except the cutout test.
fn shade(original: VertexOutput, front_facing: bool, position: vec3<f32>) -> vec4<f32> {
    var input = original;
    input.world_position = position;
    return scene_finish_shading(input, shade_material(original, front_facing, position));
}
fn shade_material(original: VertexOutput, front_facing: bool, position: vec3<f32>) -> SceneObjectMaterial {
    var input = original;
    input.world_position = position;
    let albedo_footprint = scene_atlas_footprint(albedo_texture, input);
    var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input, albedo_footprint), input.tint);
    let surface = scene_surface_channels(input);
    let auxiliary = scene_auxiliary_samples(input);

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
    let occlusion = scene_shading_occlusion(input, auxiliary.x);
    let scattering = scene_shading_scattering(input, metalness, occlusion);
    let surface_material = SceneShadingSurface(albedo_sample, normal, vec3<f32>(roughness, scattering.x, occlusion.x));
    return scene_object_material(input, geometric_normal, surface_material, scattering.y, auxiliary.y);
}
fn scene_object_material(input: VertexOutput, geometric_normal: vec3<f32>, surface: SceneShadingSurface, masked_amount: f32, emission: f32) -> SceneObjectMaterial {
    let projected = scene_project_decals(input.world_position, geometric_normal, 1u,
        surface.albedo.rgb, surface.properties.xy, surface.normal);
    let decorated = SceneShadingSurface(vec4<f32>(projected.albedo, surface.albedo.a), projected.normal,
        vec3<f32>(projected.properties, surface.properties.z));
    return SceneObjectMaterial(geometric_normal, decorated, masked_amount, emission);
}
fn scene_finish_shading(input: VertexOutput, material_surface: SceneObjectMaterial) -> vec4<f32> {
    let normal = material_surface.surface.normal;
    let albedo_sample = material_surface.surface.albedo;
    let roughness = material_surface.surface.properties.x;
    let metalness = material_surface.surface.properties.y;
    let ambient_occlusion = material_surface.surface.properties.z;
    let masked_amount = material_surface.masked_amount;
    let emission = material_surface.emission;
    let lighting = scene_surface_lighting(
        input.world_position,
        normal,
        albedo_sample.rgb,
        vec3<f32>(roughness, metalness, ambient_occlusion), DIELECTRIC_F0,
    );

    var colour = lighting.color;
    let record = scene_material_lighting_record();
    if scene_material_scatters() {
        colour += scene_record_scattering(input.world_position, normal, albedo_sample.rgb, record,
            masked_amount, sunlight_visibility(input.world_position, normal));
    }
    if scene_material_transmits() {
        colour += scene_record_transmission(normal, albedo_sample.rgb, record,
            sunlight_visibility(input.world_position, -normal));
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

@fragment
fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    if SCENE_IMPOSTOR {
        return SceneSurfaceOutput(scene_card_shade(input, front_facing, depth.position, false, depth.visible), depth.value);
    } else {
    let shaded = shade(input, front_facing, depth.position);
    scene_apply_coverage(input);
    if !depth.visible { discard; }
    return SceneSurfaceOutput(vec4<f32>(shaded.rgb, 1.0), depth.value);
    }
}


@fragment
fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    if SCENE_IMPOSTOR {
        return SceneSurfaceOutput(scene_card_shade(input, front_facing, depth.position, true, depth.visible), depth.value);
    } else {
    let shaded = shade(input, front_facing, depth.position);
    let coverage_alpha = scene_mask_alpha(shaded.a, input);
    if !scene_alpha_covered(shaded.a, input) || !depth.visible {
        discard;
    }
    return SceneSurfaceOutput(vec4<f32>(shaded.rgb, coverage_alpha), depth.value);
    }
}

@fragment
fn fs_unlit(input: VertexOutput) -> @location(0) vec4<f32> {
    let footprint = scene_atlas_footprint(albedo_texture, input);
    let sampled = scene_atlas_sample(albedo_texture, input, footprint) * input.tint;
    return vec4<f32>(sampled.rgb, scene_material_alpha(sampled.a));
}

@fragment
fn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    if SCENE_IMPOSTOR {
        return scene_card_shade(input, front_facing, scene_relief_position(input), false, true);
    } else {
    let shaded = shade(input,front_facing,scene_relief_position(input));
    scene_apply_coverage(input);
    return vec4<f32>(shaded.rgb,1.0);
    }
}
@fragment
fn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    if SCENE_IMPOSTOR {
        return scene_card_shade(input, front_facing, scene_relief_position(input), true, true);
    } else {
    let shaded = shade(input,front_facing,scene_relief_position(input));
    let coverage_alpha = scene_mask_alpha(shaded.a, input);
    if !scene_alpha_covered(shaded.a,input) { discard; }
    return vec4<f32>(shaded.rgb,coverage_alpha);
    }
}
