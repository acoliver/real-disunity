struct WaterFrame {
    clip: mat4x4<f32>, view: mat4x4<f32>, inverse_clip: mat4x4<f32>,
    origin_time: vec4<f32>, extent: vec4<f32>, color_roughness: vec4<f32>, absorption: vec4<f32>,
    sun_direction: vec4<f32>, sun_radiance: vec4<f32>, ambient: vec4<f32>, surface: vec4<f32>,
}
@group(0) @binding(0) var<uniform> water: WaterFrame;
@group(0) @binding(1) var opaque_color: texture_2d<f32>;
@group(0) @binding(2) var opaque_depth: texture_2d<f32>;
@group(0) @binding(4) var environment_cube: texture_cube<f32>;
@group(0) @binding(5) var environment_brdf: texture_2d<f32>;
@group(0) @binding(6) var environment_sampler: sampler;
@group(0) @binding(7) var surface_normal: texture_2d<f32>;
@group(0) @binding(8) var surface_flow: texture_2d<f32>;
@group(0) @binding(9) var surface_sampler: sampler;
struct WaterVertex { @builtin(position) position: vec4<f32>, @location(0) @interpolate(perspective, sample) world: vec3<f32>, }
struct WaterOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal_water: vec4<f32>,
    @location(2) material: vec4<f32>,
    @location(3) environment: vec4<f32>,
}
@vertex fn vs_water(@location(0) @interpolate(perspective, sample) world: vec3<f32>) -> WaterVertex {
    var output: WaterVertex;
    output.world = world;
    output.position = water.clip * vec4<f32>(world, 1.0);
    return output;
}
fn water_camera_transmittance(position: vec3<f32>) -> f32 {
    if frame.sun_direction.w >= 0.0 { return 1.0; }
    let origin = scene_ray_origin(position);
    let falloff = -frame.sun_direction.w - 1.0;
    let log_density = frame.camera_position.w - falloff * (origin.z - frame.camera_position.z);
    return rd_height_fog_transmittance(log_density, falloff, frame.sun_irradiance.w,
        position - origin, vec3<f32>(0.0, 0.0, 1.0));
}
@fragment fn fs_water(input: WaterVertex, @builtin(sample_index) sample: u32) -> WaterOutput {
    var velocity = water.extent.zw;
    if water.surface.w != 0.0 {
        velocity += (textureSampleLevel(surface_flow, surface_sampler, input.world.xy * water.surface.xy, 0.0).rg * 2.0 - 1.0) * water.surface.w;
    }
    let wave = input.world.xy - velocity * water.origin_time.w;
    var normal = normalize(vec3<f32>(sin(wave.x * 1.7 + water.origin_time.w) * 0.08, cos(wave.y * 2.1 + water.origin_time.w * 0.7) * 0.08, 1.0));
    if water.surface.z != 0.0 {
        let xy = textureSampleLevel(surface_normal, surface_sampler, wave * water.surface.xy, 0.0).rg * 2.0 - 1.0;
        normal = normalize(vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0001))));
    }
    let visibility = sunlight_visibility(input.world, normal);
    let size = vec2<i32>(textureDimensions(opaque_color));
    let original = clamp(vec2<i32>(input.position.xy), vec2<i32>(0), size - vec2<i32>(1));
    var refracted = clamp(original + vec2<i32>(normal.xy * 24.0 * (1.0 - water.color_roughness.w)), vec2<i32>(0), size - vec2<i32>(1));
    var surface_depth = -(water.view * vec4<f32>(input.world, 1.0)).z;
    // Preserve this invocation's sample position when moving to the candidate ray.
    let candidate_pixel = vec2<f32>(refracted) + fract(input.position.xy);
    let ndc = candidate_pixel / vec2<f32>(size) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    let near_h = water.inverse_clip * vec4<f32>(ndc, 0.0, 1.0);
    let far_h = water.inverse_clip * vec4<f32>(ndc, 1.0, 1.0);
    let near_world = near_h.xyz / near_h.w;
    let ray = far_h.xyz / far_h.w - near_world;
    let intersection = (water.origin_time.z - near_world.z) / ray.z;
    let candidate_surface = near_world + ray * intersection;
    let candidate_depth = -(water.view * vec4<f32>(candidate_surface, 1.0)).z;
    if intersection <= 0.0 || textureLoad(opaque_depth, refracted, 0).r < candidate_depth {
        refracted = original;
    } else {
        surface_depth = candidate_depth;
    }
    let distance = max(textureLoad(opaque_depth, refracted, 0).r - surface_depth, 0.0);
    let transmission = exp(-water.absorption.xyz * distance);
    let background = textureLoad(opaque_color, refracted, 0).rgb;
    let light = rd_toward_light(select(water.sun_direction.xyz, vec3<f32>(0.0, 0.0, 1.0), frame.grading.y != 0.0), false);
    let ambient = rd_hemisphere(normal, vec3<f32>(0.0, 0.0, 1.0), frame.ambient_ground.xyz, water.ambient.xyz);
    let lighting = ambient + water.sun_radiance.xyz * max(dot(normal, light), 0.0) * visibility;
    let scattering = water.color_roughness.xyz * lighting;
    let view = normalize(scene_ray_origin(input.world) - input.world);
    let no_v = clamp(dot(normal, view), 0.0001, 1.0);
    let roughness = water.color_roughness.w;
    let coefficients = textureSampleLevel(environment_brdf, environment_sampler, vec2<f32>(no_v, roughness), 0.0).rg;
    let reflected = reflect(-view, normal);
    let radiance = textureSampleLevel(environment_cube, environment_sampler, reflected, roughness * f32(textureNumLevels(environment_cube) - 1u)).rgb;
    let specular = radiance * (vec3<f32>(0.02037) * coefficients.x + vec3<f32>(coefficients.y));
    let air = water_camera_transmittance(input.world);
    let fog = vec3<f32>(frame.ambient_sky.w, frame.ambient_ground.w, frame.grading.w);
    // The snapshot already contains camera fog. Preserve its transmitted share,
    // and fog only the water's newly added scattering and reflection.
    let color = rd_compose_water_fog(background, transmission, scattering, specular, air, fog);
    return WaterOutput(vec4<f32>(color, 1.0), vec4<f32>(normalize((water.view * vec4<f32>(normal, 0.0)).xyz), 1.0), vec4<f32>(vec3<f32>(0.02037), roughness), vec4<f32>(specular * air, 1.0));
}
