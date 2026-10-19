
struct Frame {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_irradiance: vec4<f32>,
    ambient_sky: vec4<f32>,
    ambient_ground: vec4<f32>,
    // x is the linear exposure multiplier. y, z and w are reserved for the
    // contrast and saturation terms, which the domain carries and the decoder
    // reads but which do not yet reach this shader.
    grading: vec4<f32>,
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
    let alpha = roughness * roughness;
    let alpha_squared = alpha * alpha;
    let denominator = normal_dot_half * normal_dot_half * (alpha_squared - 1.0) + 1.0;
    return alpha_squared / max(PI * denominator * denominator, 1e-7);
}

// Smith height-correlated visibility term, already divided by the 4 (n.l)(n.v)
// denominator of the Cook-Torrance specular term.
fn visibility_smith(normal_dot_view: f32, normal_dot_light: f32, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha_squared = alpha * alpha;
    let view_term = normal_dot_light
        * sqrt(normal_dot_view * normal_dot_view * (1.0 - alpha_squared) + alpha_squared);
    let light_term = normal_dot_view
        * sqrt(normal_dot_light * normal_dot_light * (1.0 - alpha_squared) + alpha_squared);
    return 0.5 / max(view_term + light_term, 1e-7);
}

fn fresnel_schlick(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cosine, 0.0, 1.0), 5.0);
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
    let roughness = clamp(roughness_in, MINIMUM_ROUGHNESS, 1.0);

    let view = normalize(frame.camera_position.xyz - world_position);
    let light = normalize(frame.sun_direction.xyz);
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
    let sky_fraction = normal.z * 0.5 + 0.5;
    let ambient = mix(frame.ambient_ground.rgb, frame.ambient_sky.rgb, sky_fraction)
        * diffuse_colour
        * ambient_occlusion;

    return direct * sunlight_visibility(world_position, normal) + ambient;
}

fn apply_exposure(radiance: vec3<f32>) -> vec3<f32> {
    return radiance * frame.grading.x;
}

// Builds a tangent frame from screen-space derivatives.
//
// SceneVertex carries no tangent, so one is derived per fragment from how the
// texture coordinates change across the surface. This is the standard
// derivative-based frame; it agrees with a stored tangent basis except across
// mirrored UV seams, where the handedness a stored tangent would record is not
// recoverable from derivatives alone.
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
