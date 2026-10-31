override WATER_TERMS: u32 = 0u;
struct WaterFrame {
    clip: mat4x4<f32>, view: mat4x4<f32>, inverse_clip: mat4x4<f32>,
    origin_time: vec4<f32>, extent: vec4<f32>, color_roughness: vec4<f32>, absorption: vec4<f32>,
    sun_direction: vec4<f32>, sun_radiance: vec4<f32>, ambient: vec4<f32>, surface: vec4<f32>,
    optics: vec4<f32>, scattering: vec4<f32>, shoreline: vec4<f32>, refraction: vec4<f32>,
    tint_deep: vec4<f32>, tint_shallow: vec4<f32>, tint_controls: vec4<f32>,
    terms: vec4<f32>, environment_terms: vec4<f32>, composition: vec4<f32>,
    disk: vec4<f32>, flow_coordinates: vec4<f32>,
    wind_domain: vec4<f32>, wind_band: vec4<f32>, wind_flags: vec4<f32>,
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
@group(0) @binding(10) var wind_field: texture_2d<f32>;
struct WaterVertex { @builtin(position) position: vec4<f32>, @location(0) @interpolate(perspective, sample) world: vec3<f32>, @location(1) wind: vec2<f32>, }
struct WaterOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal_water: vec4<f32>,
    @location(2) material: vec4<f32>,
    @location(3) environment: vec4<f32>,
}
@vertex fn vs_water(@location(0) @interpolate(perspective, sample) world: vec3<f32>) -> WaterVertex {
    var output: WaterVertex;
    output.world = world;
    output.wind = vec2<f32>(0.0);
    if (WATER_TERMS & 128u) != 0u && water.wind_flags.x != 0.0 {
        let half_texel = 0.5 / vec2<f32>(textureDimensions(wind_field));
        let uv = clamp((world.xy - water.wind_domain.xy) * water.wind_domain.zw, half_texel, vec2<f32>(1.0) - half_texel);
        let force = textureSampleLevel(wind_field, surface_sampler, uv, 0.0).rg * 2.0 - 1.0;
        output.wind = force * water_wind_scale(world, bitcast<u32>(water.wind_flags.y), water.wind_band.xyz);
    }
    output.position = water.clip * vec4<f32>(world, 1.0);
    return output;
}
fn water_camera_transmittance(position: vec3<f32>) -> f32 {
    return scene_surface_transmission(position);
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
// Endpoint-centred UVs give texel-space positions t * (N - 1).
fn endpoint_seed(t: vec2<f32>) -> vec2<f32> {
    let size = vec2<i32>(textureDimensions(surface_flow));
    let uv = (clamp(t, vec2<f32>(0.0), vec2<f32>(1.0)) * vec2<f32>(size - 1) + 0.5) / vec2<f32>(size);
    let coordinate = clamp(uv * vec2<f32>(size) - 0.5, vec2<f32>(0.0), vec2<f32>(size - 1));
    let base = vec2<i32>(floor(coordinate));
    let next = min(base + 1, size - 1);
    let fraction = fract(coordinate);
    return mix(
        mix(textureLoad(surface_flow, base, 0).rg, textureLoad(surface_flow, vec2<i32>(next.x, base.y), 0).rg, fraction.x),
        mix(textureLoad(surface_flow, vec2<i32>(base.x, next.y), 0).rg,
            textureLoad(surface_flow, next, 0).rg, fraction.x), fraction.y);
}
fn flow_velocity(uv: vec2<f32>) -> vec2<f32> {
    // Interpolate velocity in float precision: sampler weight quantization is
    // amplified by the advection period before the normal map is sampled.
    if (WATER_TERMS & 64u) != 0u && water.composition.z == 2.0 {
        return endpoint_seed(uv) * 2.0 - 1.0;
    }
    return bilinear_rg(surface_flow, uv) * 2.0 - 1.0;
}

fn water_sample_normal(uv: vec2<f32>) -> vec2<f32> {
    if (WATER_TERMS & 4u) != 0u && water.terms.w != 0.0 {
        return textureSample(surface_normal, surface_sampler, uv).rg;
    }
    return bilinear_rg(surface_normal, uv);
}
fn flowing_normal(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let xy = water_sample_normal(uv) * 2.0 - 1.0;
    let normal = vec3<f32>(xy, sqrt(clamp(1.0 - dot(xy, xy), 0.0, 1.0)));
    if water.shoreline.w != 0.0 { return water_normal_at_depth(normal, depth); }
    return normal;
}
// Vlachos (2010), "Water Flow in Portal 2": the resetting phase has zero weight.
fn two_phase_normal(world: vec2<f32>, velocity: vec2<f32>, depth: f32) -> vec3<f32> {
    let phase = fract(water.origin_time.w / water.absorption.w);
    let other = fract(phase + 0.5);
    let offset = velocity * water.absorption.w * water.surface.xy;
    let uv = world * water.surface.xy;
    var blended = mix(flowing_normal(uv + offset * phase, depth),
        flowing_normal(uv + offset * other, depth), abs(1.0 - 2.0 * phase));
    if water.shoreline.w != 0.0 { blended = vec3<f32>(blended.xy * water.shoreline.y, blended.z); }
    if dot(blended, blended) == 0.0 { return vec3<f32>(0.0, 0.0, 1.0); }
    return normalize(blended);
}
@fragment fn fs_water(input: WaterVertex, @builtin(sample_index) sample: u32) -> WaterOutput {
    let size = vec2<i32>(textureDimensions(opaque_color));
    let original = clamp(vec2<i32>(input.position.xy), vec2<i32>(0), size - vec2<i32>(1));
    let original_surface_depth = -(water.view * vec4<f32>(input.world, 1.0)).z;
    let original_distance = max(textureLoad(opaque_depth, original, 0).r - original_surface_depth, 0.0);
    var shore_depth = 0.0;
    if water.shoreline.w != 0.0 { shore_depth = original_distance / water.shoreline.x; }
    var velocity = water.extent.zw;
    var flow_uv = input.world.xy * water.surface.xy;
    if (WATER_TERMS & 64u) != 0u && water.composition.z != 0.0 {
        flow_uv = (input.world.xy - water.flow_coordinates.xy) * water.flow_coordinates.zw;
    }
    if water.surface.w != 0.0 {
        if water.absorption.w > 0.0 || ((WATER_TERMS & 64u) != 0u && water.composition.z == 2.0) {
            velocity += flow_velocity(flow_uv) * water.surface.w;
        } else {
            velocity += (textureSampleLevel(surface_flow, surface_sampler, flow_uv, 0.0).rg * 2.0 - 1.0) * water.surface.w;
        }
    }
    if (WATER_TERMS & 128u) != 0u && water.wind_flags.x != 0.0 && water.wind_band.w != 0.0 {
        let seed = flow_velocity(flow_uv);
        let driven = water_wind_seed(seed, input.wind) / (water.absorption.w * water.surface.xy);
        velocity += (driven - seed * water.surface.w) * water.wind_band.w;
    }
    if water.shoreline.w != 0.0 { velocity *= water_flow_at_depth(shore_depth); }
    let wave = input.world.xy - velocity * water.origin_time.w;
    var normal = normalize(vec3<f32>(sin(wave.x * 1.7 + water.origin_time.w) * 0.08, cos(wave.y * 2.1 + water.origin_time.w * 0.7) * 0.08, 1.0));
    if water.surface.z != 0.0 {
        if water.absorption.w > 0.0 {
            normal = two_phase_normal(input.world.xy, velocity, shore_depth);
        } else {
            let xy = water_sample_normal(wave * water.surface.xy) * 2.0 - 1.0;
            normal = normalize(vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0001))));
        }
    }
    if water.shoreline.w != 0.0 && water.absorption.w == 0.0 {
        let shaped = water_normal_at_depth(normal, shore_depth);
        let adjusted = vec3<f32>(shaped.xy * water.shoreline.y, shaped.z);
        normal = vec3<f32>(0.0, 0.0, 1.0);
        if dot(adjusted, adjusted) > 0.0 { normal = normalize(adjusted); }
    }

    if (WATER_TERMS & 128u) != 0u && water.wind_flags.x != 0.0 && water.wind_band.w != 0.0 {
        let gain = mix(1.0, (input.wind.x + input.wind.y) * 0.5, water.wind_band.w);
        let adjusted = vec3<f32>(normal.xy * gain, normal.z);
        normal = vec3<f32>(0.0, 0.0, 1.0);
        if dot(adjusted, adjusted) > 0.0 { normal = normalize(adjusted); }
    }
    var refracted = clamp(original + vec2<i32>(normal.xy * 24.0 * (1.0 - water.color_roughness.w)), vec2<i32>(0), size - vec2<i32>(1));
    if water.refraction.y != 0.0 {
        let offset = water_refraction_offset(normal.xy, original_distance, water.refraction.x);
        let displaced_clip = water.clip * vec4<f32>(input.world + vec3<f32>(offset, 0.0), 1.0);
        let surface_clip = water.clip * vec4<f32>(input.world, 1.0);
        refracted = original;
        if displaced_clip.w > 0.0 && displaced_clip.z >= 0.0 {
            let pixels = (displaced_clip.xy / displaced_clip.w - surface_clip.xy / surface_clip.w)
                * vec2<f32>(0.5, -0.5) * vec2<f32>(size);
            refracted = clamp(original + vec2<i32>(pixels), vec2<i32>(0), size - vec2<i32>(1));
        }
    }
    var surface_depth = original_surface_depth;
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
    var optical_distance = distance;
    if water.optics.z != 0.0 || water.refraction.y != 0.0 || water.refraction.w != 0.0 || water.shoreline.w != 0.0 { optical_distance = original_distance; }
    let transmission = exp(-water.absorption.xyz * optical_distance);
    var background = textureLoad(opaque_color, refracted, 0).rgb;
    if water.refraction.w != 0.0 {
        let offset = water_refraction_offset(normal.xy, original_distance, water.refraction.z);
        background = water_screen_background(original, offset, fract(input.position.xy));
    }
    if (WATER_TERMS & 1u) != 0u && water.terms.y != 0.0 {
        let adjusted = vec3<f32>(normal.xy * water.terms.x, normal.z);
        normal = vec3<f32>(0.0, 0.0, 1.0);
        if dot(adjusted, adjusted) > 0.0 { normal = normalize(adjusted); }
    }
    let visibility = sunlight_visibility(input.world, normal);
    let light = rd_toward_light(select(water.sun_direction.xyz, vec3<f32>(0.0, 0.0, 1.0), frame.grading.y != 0.0), false);
    let ambient = scene_diffuse_environment(normal, frame.ambient_ground.xyz, water.ambient.xyz);
    let lighting = ambient + water.sun_radiance.xyz * max(dot(normal, light), 0.0) * visibility;
    var base_color = water.color_roughness.xyz;
    if water.tint_deep.w != 0.0 {
        base_color = water_tint_color(background, original_distance, water.tint_deep.xyz,
            water.tint_shallow.xyz, water.tint_controls);
    }
    var scattering = base_color * lighting;
    if (WATER_TERMS & 2u) != 0u { scattering *= 1.0 - water.terms.z; }
    let view = normalize(scene_ray_origin(input.world) - input.world);
    let no_v = clamp(dot(normal, view), 0.0001, 1.0);
    let roughness = water.color_roughness.w;
    var environment_roughness = roughness;
    if SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u {
        environment_roughness = scene_environment_roughness(roughness);
    }
    let coefficients = textureSampleLevel(environment_brdf, environment_sampler, vec2<f32>(no_v, environment_roughness), 0.0).rg;
    let reflected = reflect(-view, normal);
    let radiance = textureSampleLevel(environment_cube, environment_sampler, reflected, environment_roughness * f32(textureNumLevels(environment_cube) - 1u)).rgb * water.ambient.w;
    var f0 = 0.02037;
    if water.optics.z != 0.0 { f0 = water.optics.x; }
    let lighting_f0 = select(vec3<f32>(f0), vec3<f32>(0.04), (WATER_TERMS & 2u) != 0u && water.composition.y != 0.0);
    var material_f0 = lighting_f0;
    if (WATER_TERMS & 2u) != 0u { material_f0 = mix(material_f0, base_color, water.terms.z); }
    var environment_specular = radiance * rd_split_sum(material_f0, coefficients);
    if SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u {
        environment_specular = radiance * scene_environment_factor(material_f0, coefficients, no_v, 1.0, environment_roughness);
    }
    var specular = environment_specular;
    if (SCENE_ROUGHNESS_REMAP != 0u || SCENE_SPECULAR_VISIBILITY != 0u || SCENE_SPECULAR_OCCLUSION != 0u || SCENE_DIRECT_DIFFUSE != 0u) && frame.grading.y == 0.0 {
        let lit = scene_surface_lighting(input.world, normal, base_color,
            vec3<f32>(roughness, water.terms.z, 1.0), lighting_f0);
        scattering = lit.diffuse;
        if (WATER_TERMS & 32u) == 0u || water.composition.w == 0.0 { specular += lit.specular; }
    }
    if (WATER_TERMS & 32u) != 0u && water.composition.w != 0.0 && frame.grading.y == 0.0 {
        specular += water_disk_highlight(normal, view, light, material_f0, roughness, visibility);
    }
    var reflection_gain = 1.0;
    if water.optics.z != 0.0 {
        let gain = water.optics.y * water.optics.y;
        if (WATER_TERMS & 8u) != 0u && water.environment_terms.w != 0.0 {
            specular = (specular - environment_specular) * gain + environment_specular;
        } else {
            reflection_gain = gain;
            specular *= gain;
            environment_specular *= gain;
        }
    }
    if (WATER_TERMS & 8u) != 0u && water.environment_terms.w != 0.0 {
        let gain = water_environment_gain(input.world.xy, original_distance);
        specular += environment_specular * (gain - 1.0);
        environment_specular *= gain;
        reflection_gain *= gain;
    }
    let air = water_camera_transmittance(input.world);
    let fog = vec3<f32>(frame.ambient_sky.w, frame.ambient_ground.w, frame.grading.w);
    // The snapshot already contains camera fog. Preserve its transmitted share,
    // and fog only the water's newly added scattering and reflection.
    var color = rd_compose_water_fog(background, transmission, scattering, specular, air, fog);
    if (WATER_TERMS & 16u) != 0u && water.composition.x != 0.0 {
        color = (scattering + specular) * air + fog * (1.0 - air);
    }
    if water.optics.z != 0.0 { color += water_view_scattering(view, water.scattering.xyz) * air; }
    if water.shoreline.w != 0.0 {
        let opacity = water_opacity_at_depth(shore_depth) * water.shoreline.z;
        color = mix(textureLoad(opaque_color, original, 0).rgb, color, opacity);
        environment_specular *= opacity;
        reflection_gain *= opacity;
    }
    let receiver_clip = water.clip * vec4<f32>(input.world, 1.0);
    let receiver_pixel = (receiver_clip.xy / receiver_clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5)) * vec2<f32>(size);
    var receiver_normal = vec4<f32>(normalize((water.view * vec4<f32>(normal, 0.0)).xyz), 1.0);
    if (WATER_TERMS & 2u) != 0u && water.composition.y != 0.0 {
        let encoded = water_pack_normal(receiver_normal.xyz);
        receiver_normal = vec4<f32>(encoded, material_f0.x, -1.0 - material_f0.z);
        f0 = material_f0.y;
    }
    return WaterOutput(vec4<f32>(color, 1.0), receiver_normal, vec4<f32>(receiver_pixel - floor(input.position.xy), f0, environment_roughness), vec4<f32>(environment_specular * air, air * reflection_gain));
}
