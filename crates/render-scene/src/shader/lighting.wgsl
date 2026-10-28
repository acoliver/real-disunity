override SCENE_RASTER_MIRRORED: bool = false;
override SCENE_DIRECT_DIFFUSE: u32 = 0u;
override SCENE_DISK_INTENSITY: f32 = 0.0;
override SCENE_DISK_RADIUS: f32 = 0.0027;
override SCENE_DISK_PEAK: f32 = 0.0;

override SCENE_ROUGHNESS_REMAP: u32 = 0u;
override SCENE_SPECULAR_VISIBILITY: u32 = 0u;
override SCENE_SPECULAR_OCCLUSION: u32 = 0u;
struct Frame {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_irradiance: vec4<f32>,
    ambient_sky: vec4<f32>,
    ambient_ground: vec4<f32>,
    // x is exposure; y selects unlit rendering when no environment is authored.
    // z holds the animation clock as u32 bits; w is reserved and zero.
    grading: vec4<f32>,
    diffuse_sh: array<vec4<f32>, 9>,
    // x selects SH; y stores untinted sun intensity when z selects a coloured sun.
    // Neutral sun frames leave y/z zero and retain intensity in sun_irradiance.x.
    lighting_options: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

// Reflectance of a dielectric at normal incidence. Metals take their F0 from
// the albedo instead, which is what the metalness parameter interpolates.
const DIELECTRIC_F0: vec3<f32> = vec3<f32>(0.04, 0.04, 0.04);

const PI: f32 = 3.14159265;

// A perfectly smooth surface concentrates the highlight into a point the
// sampling never lands on, leaving it invisible rather than sharp.
const MINIMUM_ROUGHNESS: f32 = 0.045;

// GGX normal distribution.
fn distribution_ggx(normal_dot_half: f32, roughness: f32) -> f32 {
    return rd_distribution_ggx(normal_dot_half, roughness, 1e-7, true);
}
fn scene_distribution_ggx(normal_dot_half: f32, roughness: f32) -> f32 {
    if SCENE_ROUGHNESS_REMAP != 0u {
        return rd_distribution_ggx(normal_dot_half, roughness, 1e-8, false);
    }
    return rd_distribution_ggx(normal_dot_half, roughness, 1e-7, true);
}

// Smith height-correlated visibility term, already divided by the 4 (n.l)(n.v)
// denominator of the Cook-Torrance specular term.
fn visibility_smith(normal_dot_view: f32, normal_dot_light: f32, roughness: f32) -> f32 {
    return rd_visibility(normal_dot_view, normal_dot_light, roughness, 0u);
}

fn fresnel_schlick(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    return rd_fresnel_schlick(cosine, f0);
}

// Prepared E/pi polynomial, shared by all material passes in native Z-up space.
fn scene_diffuse_environment(normal: vec3<f32>, ground: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    if frame.lighting_options.x == 0.0 {
        return rd_hemisphere(normal, vec3<f32>(0.0, 0.0, 1.0), ground, sky);
    }
    var coefficients: array<vec3<f32>, 9>;
    for (var i = 0u; i < 9u; i++) { coefficients[i] = frame.diffuse_sh[i].xyz; }
    return max(vec3<f32>(0.0), rd_irradiance_polynomial(normal, coefficients));
}

// Resolves one surface's outgoing radiance from the sun and the ambient
// hemisphere. Emission is deliberately excluded: it is the one term that is not
// a property of the lighting environment, and terrain has none.
fn surface_lighting(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    albedo: vec3<f32>,
    roughness_in: f32,
    metalness: f32,
    ambient_occlusion: f32,
) -> vec3<f32> {
    return scene_surface_lighting(world_position, normal, albedo,
        vec3<f32>(roughness_in, metalness, ambient_occlusion), DIELECTRIC_F0).color;
}
struct SceneSurfaceLight { color: vec3<f32>, diffuse: vec3<f32>, specular: vec3<f32>, }
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

    let f0 = mix(dielectric_f0, albedo, metalness);
    let fresnel = fresnel_schlick(view_dot_half, f0);
    let specular = scene_specular_brdf(vec4<f32>(normal_dot_view, normal_dot_light, normal_dot_half, view_dot_half), roughness, fresnel);

    // Metals have no diffuse lobe; their absorbed light does not re-emerge.
    let diffuse_colour = albedo * (1.0 - metalness);
    var diffuse = (vec3<f32>(1.0) - fresnel) * diffuse_colour / PI;
    if SCENE_DIRECT_DIFFUSE != 0u { diffuse = diffuse_colour; }

    let shadow = sunlight_visibility(world_position, normal);
    let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, shadow);
    var environment_roughness = roughness;
    if SCENE_ROUGHNESS_REMAP != 0u {
        environment_roughness = rd_remap_roughness(roughness_in, SCENE_ROUGHNESS_REMAP);
    }
    let specular_ao = scene_specular_ao(normal_dot_view, ao, environment_roughness);
    let direct = (diffuse + specular) * frame.sun_irradiance.rgb * normal_dot_light;

    // Hemisphere ambient about the scene's own up axis. Scene space is Z-up, so
    // the sky/ground blend reads the z component; using y here would light the
    // world as though the horizon ran vertically.
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

fn apply_exposure(radiance: vec3<f32>) -> vec3<f32> {
    return rd_post_exposure(radiance, frame.grading.x);
}

// Builds a tangent frame from screen-space derivatives.
//
// Vertices without authored tangents retain the derivative-based frame. Stored
// tangents supply the independently authored orientation and handedness instead.
fn cotangent_frame(normal: vec3<f32>, position: vec3<f32>, uv: vec2<f32>) -> mat3x3<f32> {
    let position_dx = dpdx(position);
    let position_dy = dpdy(position);
    let uv_dx = dpdx(uv);
    let uv_dy = dpdy(uv);

    let perpendicular_dy = cross(position_dy, normal);
    let perpendicular_dx = cross(normal, position_dx);
    let tangent = perpendicular_dy * uv_dx.x + perpendicular_dx * uv_dy.x;
    let bitangent = perpendicular_dy * uv_dx.y + perpendicular_dx * uv_dy.y;

    var scale = inverseSqrt(max(dot(tangent, tangent), dot(bitangent, bitangent)));
    // A mirrored raster reverses both derivative axes of the cotangent frame.
    if SCENE_RASTER_MIRRORED { scale = -scale; }
    return mat3x3<f32>(tangent * scale, bitangent * scale, normal);
}

fn apply_height_fog(radiance: vec3<f32>, position: vec3<f32>) -> vec3<f32> {
    if frame.sun_direction.w >= 0.0 { return radiance; }
    let transmittance = scene_surface_transmission(position);
    return mix(vec3(frame.ambient_sky.w, frame.ambient_ground.w, frame.grading.w), radiance, transmittance);
}

// The distorted light vector is deliberately left unnormalised.
fn scene_backlight_strength(ray: vec3<f32>, light: vec3<f32>, normal: vec3<f32>,
    shape: vec4<f32>, factors: vec3<f32>) -> f32 {
    let alignment = clamp(dot(ray, light - normal * shape.x), 0.0, 1.0);
    return (pow(alignment, shape.y) * shape.z + shape.w) * factors.x * factors.y * factors.z;
}

fn scene_temperature_rgb(kelvin: f32) -> vec3<f32> {
    let t = kelvin / 100.0;
    var rgb = vec3<f32>(0.0);
    if t <= 66.0 {
        rgb.x = 255.0;
        rgb.y = 99.4708025861 * log(t) - 161.1195681661;
    } else {
        rgb.x = 329.698727446 * pow(t - 60.0, -0.1332047592);
        rgb.y = 288.1221695283 * pow(t - 60.0, -0.0755148492);
    }
    if t >= 66.0 { rgb.z = 255.0; }
    else if t > 19.0 { rgb.z = 138.5177312231 * log(t - 10.0) - 305.0447927307; }
    return clamp(rgb / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn scene_scattering_tint(base: vec3<f32>, colour: vec4<f32>, warming: f32) -> vec3<f32> {
    let neutral = max(vec3<f32>(0.001), scene_temperature_rgb(colour.w));
    let warm = scene_temperature_rgb(max(1000.0, colour.w - warming));
    var tint = base / neutral * warm;
    let weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    tint = tint * colour.y / max(0.0001, dot(tint, weights));
    return mix(tint, vec3<f32>(dot(tint, weights)), -colour.z);
}

fn scene_roughness(value: f32) -> f32 {
    if SCENE_ROUGHNESS_REMAP != 0u {
        return clamp(rd_remap_roughness(value, SCENE_ROUGHNESS_REMAP), 0.04, 1.0);
    }
    return clamp(value, MINIMUM_ROUGHNESS, 1.0);
}
fn scene_screen_ao(world_position: vec3<f32>) -> f32 {
    return 1.0 /* SCREEN_AMBIENT_FACTOR */;
}
fn scene_ao(material_ao: f32, world_position: vec3<f32>, no_l: f32, shadow: f32) -> f32 {
    let screen = scene_screen_ao(world_position);
    if SCENE_SPECULAR_OCCLUSION == 0u { return material_ao * screen; }
    let combined = min(material_ao, screen * 1.01);
    let sun_intensity = select(frame.sun_irradiance.x, frame.lighting_options.y,
        frame.lighting_options.z != 0.0);
    let relief = clamp(min(shadow, no_l) * min(1.0, sun_intensity / 3.0), 0.0, 1.0);
    return mix(combined, 1.0, relief);
}
fn scene_specular_ao(no_v: f32, ao: f32, roughness: f32) -> f32 {
    if SCENE_SPECULAR_OCCLUSION == 0u { return 1.0; }
    return rd_specular_occlusion(no_v, ao, roughness);
}
fn scene_complete_lighting(direct: vec3<f32>, shadow: f32, ambient: vec3<f32>, environment: vec3<f32>) -> vec3<f32> {
    return direct * shadow + ambient + environment;
}

fn scene_specular_brdf(dots: vec4<f32>, roughness: f32, fresnel: vec3<f32>) -> vec3<f32> {
    return fresnel * scene_distribution_ggx(dots.z, roughness)
        * rd_visibility(dots.x, dots.y, roughness, SCENE_SPECULAR_VISIBILITY);
}

// Environment coordinates are shared by forward shading and receiver publication.
fn scene_environment_roughness(value: f32) -> f32 {
    if SCENE_ROUGHNESS_REMAP == 0u { return scene_roughness(value); }
    return rd_remap_roughness(value, SCENE_ROUGHNESS_REMAP);
}
fn scene_environment_factor(f0: vec3<f32>, coefficients: vec2<f32>, no_v: f32, ao: f32, roughness: f32) -> vec3<f32> {
    return rd_split_sum(f0, coefficients) * scene_specular_ao(no_v, ao, roughness);
}

fn scene_limit_highlight(value: vec3<f32>, peak: f32) -> vec3<f32> {
    let luminance = dot(value, vec3<f32>(0.2126, 0.7152, 0.0722));
    if luminance <= peak { return value; }
    return value * (peak / luminance);
}
fn scene_disk_highlight(normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, f0: vec3<f32>, roughness: f32, shadow: f32) -> vec3<f32> {
    let selected = scene_disk_direction(reflect(-view, normal), light, SCENE_DISK_RADIUS);
    let half_vector = normalize(view + selected);
    let dots = vec4<f32>(clamp(dot(normal, view), 0.0001, 1.0),
        max(dot(normal, selected), 0.0), max(dot(normal, half_vector), 0.0), max(dot(view, half_vector), 0.0));
    var alpha = max(roughness, 0.04);
    if SCENE_ROUGHNESS_REMAP != 0u { alpha = max(scene_environment_roughness(roughness), 0.04); }
    let highlight = scene_specular_brdf(dots, alpha, fresnel_schlick(dots.w, f0))
        * SCENE_DISK_INTENSITY * scene_sun_tint() * max(dot(normal, light), 0.0) * shadow;
    return scene_limit_highlight(highlight, SCENE_DISK_PEAK);
}

fn scene_surface_transmission(position:vec3<f32>)->f32 {
    if frame.sun_direction.w >= 0.0 { return 1.0; }
    let origin=scene_ray_origin(position);
    let falloff=-frame.sun_direction.w-1.0;
    let density=frame.camera_position.w-falloff*(origin.z-frame.camera_position.z);
    if frame.diffuse_sh[0].w != 0.0 {
        return rd_full_ray_fog_transmittance(density,falloff,frame.sun_irradiance.w,position-origin,vec3(0.0,0.0,1.0));
    }
    return rd_height_fog_transmittance(density,falloff,frame.sun_irradiance.w,position-origin,vec3(0.0,0.0,1.0));
}

fn scene_sun_tint()->vec3<f32> {
    if frame.lighting_options.z == 0.0 { return vec3(1.0); }
    return vec3(frame.diffuse_sh[2].w,frame.diffuse_sh[3].w,frame.diffuse_sh[4].w);
}
