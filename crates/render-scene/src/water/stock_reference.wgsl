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
fn texel_wrapped(source: texture_2d<f32>, pixel: vec2<i32>, size: vec2<i32>) -> vec2<f32> {
    let wrapped = ((pixel % size) + size) % size;
    return textureLoad(source, wrapped, 0).rg;
}
// Level-0 repeat-wrapped bilinear filter in float precision. Vulkan leaves
// sampler filter precision implementation-defined: some software samplers
// quantize the weights and round the filtered 8-bit result.
fn bilinear_rg(source: texture_2d<f32>, uv: vec2<f32>) -> vec2<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let coordinate = fract(uv) * vec2<f32>(size) - 0.5;
    let base = vec2<i32>(floor(coordinate));
    let fraction = fract(coordinate);
    return mix(
        mix(texel_wrapped(source, base, size), texel_wrapped(source, base + vec2<i32>(1, 0), size), fraction.x),
        mix(texel_wrapped(source, base + vec2<i32>(0, 1), size),
            texel_wrapped(source, base + vec2<i32>(1, 1), size), fraction.x), fraction.y);
}
fn flow_velocity(uv: vec2<f32>) -> vec2<f32> {
    // Interpolate velocity in float precision: sampler weight quantization is
    // amplified by the advection period before the normal map is sampled.
    return bilinear_rg(surface_flow, uv) * 2.0 - 1.0;
}

fn flowing_normal(uv: vec2<f32>) -> vec3<f32> {
    let xy = bilinear_rg(surface_normal, uv) * 2.0 - 1.0;
    return vec3<f32>(xy, sqrt(clamp(1.0 - dot(xy, xy), 0.0, 1.0)));
}
// Vlachos (2010), "Water Flow in Portal 2": the resetting phase has zero weight.
fn two_phase_normal(world: vec2<f32>, velocity: vec2<f32>) -> vec3<f32> {
    let phase = fract(water.origin_time.w / water.absorption.w);
    let other = fract(phase + 0.5);
    let offset = velocity * water.absorption.w * water.surface.xy;
    let uv = world * water.surface.xy;
    let blended = mix(flowing_normal(uv + offset * phase),
        flowing_normal(uv + offset * other), abs(1.0 - 2.0 * phase));
    if dot(blended, blended) == 0.0 { return vec3<f32>(0.0, 0.0, 1.0); }
    return normalize(blended);
}
@fragment fn fs_water(input: WaterVertex, @builtin(sample_index) sample: u32) -> WaterOutput {
    var velocity = water.extent.zw;
    if water.surface.w != 0.0 {
        if water.absorption.w > 0.0 {
            velocity += flow_velocity(input.world.xy * water.surface.xy) * water.surface.w;
        } else {
            velocity += (textureSampleLevel(surface_flow, surface_sampler, input.world.xy * water.surface.xy, 0.0).rg * 2.0 - 1.0) * water.surface.w;
        }
    }
    let wave = input.world.xy - velocity * water.origin_time.w;
    var normal = normalize(vec3<f32>(sin(wave.x * 1.7 + water.origin_time.w) * 0.08, cos(wave.y * 2.1 + water.origin_time.w * 0.7) * 0.08, 1.0));
    if water.surface.z != 0.0 {
        if water.absorption.w > 0.0 {
            normal = two_phase_normal(input.world.xy, velocity);
        } else {
            let xy = bilinear_rg(surface_normal, wave * water.surface.xy) * 2.0 - 1.0;
            normal = normalize(vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0001))));
        }
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
    let ambient = scene_diffuse_environment(normal, frame.ambient_ground.xyz, water.ambient.xyz);
    let lighting = ambient + water.sun_radiance.xyz * max(dot(normal, light), 0.0) * visibility;
    var scattering = water.color_roughness.xyz * lighting;
    let view = normalize(scene_ray_origin(input.world) - input.world);
    let no_v = clamp(dot(normal, view), 0.0001, 1.0);
    let roughness = water.color_roughness.w;
    var environment_roughness = roughness;
    if SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u {
        environment_roughness = scene_environment_roughness(roughness);
    }
    let coefficients = textureSampleLevel(environment_brdf, environment_sampler, vec2<f32>(no_v, environment_roughness), 0.0).rg;
    let reflected = reflect(-view, normal);
    let radiance = textureSampleLevel(environment_cube, environment_sampler, reflected, environment_roughness * f32(textureNumLevels(environment_cube) - 1u)).rgb;
    var environment_specular = radiance * rd_split_sum(vec3<f32>(0.02037), coefficients);
    if SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u {
        environment_specular = radiance * scene_environment_factor(vec3<f32>(0.02037), coefficients, no_v, 1.0, environment_roughness);
    }
    var specular = environment_specular;
    if (SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u) && frame.grading.y == 0.0 {
        let lit = scene_surface_lighting(input.world, normal, water.color_roughness.xyz,
            vec3<f32>(roughness, 0.0, 1.0), vec3<f32>(0.02037));
        scattering = lit.diffuse;
        specular += lit.specular;
    }
    let air = water_camera_transmittance(input.world);
    let fog = vec3<f32>(frame.ambient_sky.w, frame.ambient_ground.w, frame.grading.w);
    // The snapshot already contains camera fog. Preserve its transmitted share,
    // and fog only the water's newly added scattering and reflection.
    let color = rd_compose_water_fog(background, transmission, scattering, specular, air, fog);
    let receiver_clip = water.clip * vec4<f32>(input.world, 1.0);
    let receiver_pixel = (receiver_clip.xy / receiver_clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5)) * vec2<f32>(size);
    return WaterOutput(vec4<f32>(color, 1.0), vec4<f32>(normalize((water.view * vec4<f32>(normal, 0.0)).xyz), 1.0), vec4<f32>(receiver_pixel - floor(input.position.xy), 0.02037, environment_roughness), vec4<f32>(environment_specular * air, air));
}
