// Surface-lighting reference from cd26906e, engine MIT source.
fn scene_surface_lighting(world_position: vec3<f32>, normal: vec3<f32>, albedo: vec3<f32>, properties: vec3<f32>, dielectric_f0: vec3<f32>) -> SceneSurfaceLight {
    if frame.grading.y != 0.0 { return SceneSurfaceLight(albedo, albedo, vec3<f32>(0.0)); }
    let roughness_in = properties.x;
    let metalness = properties.y;
    let ambient_occlusion = properties.z;
    let roughness = scene_roughness(roughness_in);
    let view = normalize(frame.camera_position.xyz - world_position);
    let light = rd_toward_light(frame.sun_direction.xyz, false);
    let half_vector = normalize(view + light);
    let normal_dot_view = clamp(dot(normal, view), 1e-4, 1.0);
    let normal_dot_light = clamp(dot(normal, light), 0.0, 1.0);
    let normal_dot_half = clamp(dot(normal, half_vector), 0.0, 1.0);
    let view_dot_half = clamp(dot(view, half_vector), 0.0, 1.0);
    let f0 = scene_material_f0(albedo, metalness, dielectric_f0);
    let fresnel = fresnel_schlick(view_dot_half, f0);
    let specular = scene_specular_brdf(vec4<f32>(normal_dot_view, normal_dot_light, normal_dot_half, view_dot_half), roughness, fresnel);
    let diffuse_colour = albedo * (1.0 - metalness);
    var diffuse = (vec3<f32>(1.0) - fresnel) * diffuse_colour / PI;
    if SCENE_DIRECT_DIFFUSE != 0u { diffuse = diffuse_colour; }
    let visibility = raw_sunlight_visibility(world_position, normal);
    let shadow = direct_sunlight_visibility(visibility);
    let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, visibility);
    var environment_roughness = roughness;
    if SCENE_ROUGHNESS_REMAP != 0u {
        environment_roughness = rd_remap_roughness(roughness_in, SCENE_ROUGHNESS_REMAP);
    }
    let specular_ao = scene_specular_ao(normal_dot_view, ao, environment_roughness);
    let direct = (diffuse + specular) * frame.sun_irradiance.rgb * normal_dot_light;
    let irradiance = scene_diffuse_environment(normal, frame.ambient_ground.rgb, frame.ambient_sky.rgb);
    var ambient = irradiance * diffuse_colour * ambient_occlusion * scene_screen_ao(world_position);
    if SCENE_SPECULAR_OCCLUSION != 0u { ambient = irradiance * diffuse_colour * ao; }
    let environment = vec3<f32>(0.0); /* SCENE_ENVIRONMENT */
    var color = scene_complete_lighting(direct, shadow, ambient, environment);
    if SCENE_SPECULAR_OCCLUSION != 0u {
        let occluded_direct = (diffuse * ao + specular * specular_ao) * frame.sun_irradiance.rgb * normal_dot_light;
        color = scene_complete_lighting(occluded_direct, shadow, ambient, environment);
    }
    var diffuse_ao = 1.0;
    if SCENE_SPECULAR_OCCLUSION != 0u { diffuse_ao = ao; }
    let diffuse_radiance = diffuse * diffuse_ao
        * frame.sun_irradiance.rgb * normal_dot_light * shadow + ambient;
    let specular_radiance = specular * specular_ao * frame.sun_irradiance.rgb * normal_dot_light * shadow + environment;
    if SCENE_DISK_PEAK > 0.0 {
        let disk = scene_disk_highlight(normal, view, light, f0, roughness_in, shadow) * specular_ao;
        let selected_diffuse = diffuse * diffuse_ao * frame.sun_irradiance.rgb * normal_dot_light * shadow + ambient;
        return SceneSurfaceLight(selected_diffuse + disk + environment, selected_diffuse, disk + environment);
    }
    return SceneSurfaceLight(color, diffuse_radiance, specular_radiance);
}
