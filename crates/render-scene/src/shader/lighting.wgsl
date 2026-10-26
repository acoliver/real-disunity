
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
    if frame.grading.y != 0.0 {
        return albedo;
    }
    let roughness = clamp(roughness_in, MINIMUM_ROUGHNESS, 1.0);

    let view = normalize(frame.camera_position.xyz - world_position);
    let light = rd_toward_light(frame.sun_direction.xyz, false);
    let half_vector = normalize(view + light);

    let normal_dot_view = clamp(dot(normal, view), 1e-4, 1.0);
    let normal_dot_light = clamp(dot(normal, light), 0.0, 1.0);
    let normal_dot_half = clamp(dot(normal, half_vector), 0.0, 1.0);
    let view_dot_half = clamp(dot(view, half_vector), 0.0, 1.0);

    let f0 = mix(DIELECTRIC_F0, albedo, metalness);
    let fresnel = fresnel_schlick(view_dot_half, f0);
    let specular = fresnel
        * distribution_ggx(normal_dot_half, roughness)
        * visibility_smith(normal_dot_view, normal_dot_light, roughness);

    // Metals have no diffuse lobe; their absorbed light does not re-emerge.
    let diffuse_colour = albedo * (1.0 - metalness);
    let diffuse = (vec3<f32>(1.0) - fresnel) * diffuse_colour / PI;

    let direct = (diffuse + specular) * frame.sun_irradiance.rgb * normal_dot_light;

    // Hemisphere ambient about the scene's own up axis. Scene space is Z-up, so
    // the sky/ground blend reads the z component; using y here would light the
    // world as though the horizon ran vertically.
    let ambient = scene_diffuse_environment(normal, frame.ambient_ground.rgb, frame.ambient_sky.rgb)
        * diffuse_colour
        * ambient_occlusion /* SCREEN_AMBIENT_FACTOR */;

    return direct * sunlight_visibility(world_position, normal) + ambient;
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

    let scale = inverseSqrt(max(dot(tangent, tangent), dot(bitangent, bitangent)));
    return mat3x3<f32>(tangent * scale, bitangent * scale, normal);
}

fn apply_height_fog(radiance: vec3<f32>, position: vec3<f32>) -> vec3<f32> {
    if frame.sun_direction.w >= 0.0 { return radiance; }
    let origin = scene_ray_origin(position);
    let falloff = -frame.sun_direction.w - 1.0;
    let log_density = frame.camera_position.w - falloff * (origin.z - frame.camera_position.z);
    let transmittance = rd_height_fog_transmittance(log_density, falloff,
        frame.sun_irradiance.w, position - origin, vec3(0.0, 0.0, 1.0));
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
