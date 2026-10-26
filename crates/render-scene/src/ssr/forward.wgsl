@group(3) @binding(1) var reflection_environment: texture_cube<f32>;
@group(3) @binding(2) var reflection_brdf: texture_2d<f32>;
@group(3) @binding(3) var reflection_sampler: sampler;
var<private> published_environment: vec3<f32>;
var<private> published_factor: vec3<f32>;
var<private> published_self_ao: f32;
var<private> published_visibility: f32;
struct ReflectionOutput {
    @location(0) color: vec4<f32>,
    @location(1) environment: vec4<f32>,
    @location(2) factor: vec4<f32>,
}
fn forward_reflection(world: vec3<f32>, normal: vec3<f32>, f0: vec3<f32>, roughness: f32, ao: f32, self_ao: f32) -> vec3<f32> {
    let view = normalize(scene_ray_origin(world) - world);
    let cosine = clamp(dot(normal, view), 0.0001, 1.0);
    let coefficients = textureSampleLevel(reflection_brdf, reflection_sampler, vec2<f32>(cosine, roughness), 0.0).rg;
    let factor = scene_environment_factor(f0, coefficients, cosine, ao, roughness);
    let radiance = textureSampleLevel(reflection_environment, reflection_sampler, reflect(-view, normal), roughness * f32(textureNumLevels(reflection_environment) - 1u)).rgb;
    var air = 1.0;
    if frame.sun_direction.w < 0.0 {
        let origin = scene_ray_origin(world);
        let falloff = -frame.sun_direction.w - 1.0;
        let density = frame.camera_position.w - falloff * (origin.z - frame.camera_position.z);
        air = rd_height_fog_transmittance(density, falloff, frame.sun_irradiance.w, world - origin, vec3<f32>(0.0, 0.0, 1.0));
    }
    published_self_ao = self_ao;
    published_visibility = cosine;
    published_factor = factor * air;
    published_environment = radiance * published_factor * frame.grading.x;
    // APPLICATION_FACTOR
    return radiance * factor;
}
fn reflection_output(color: vec4<f32>) -> ReflectionOutput {
    return ReflectionOutput(color, vec4<f32>(published_environment, 1.0), vec4<f32>(published_factor, 1.0));
}
