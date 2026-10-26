struct ReflectionCamera {
    projection: mat4x4<f32>, inverse_projection: mat4x4<f32>,
    settings: vec4<f32>, size: vec4<f32>,
    traversal: vec4<f32>, limits: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: ReflectionCamera;
@group(0) @binding(1) var material: texture_2d<f32>;
@group(0) @binding(2) var normals: texture_2d<f32>;
@group(0) @binding(3) var opaque_depth: texture_2d<f32>;
@group(0) @binding(4) var final_depth: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>, @builtin(sample_index) sample: u32) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pixel.xy);
    let surface = textureLoad(material, p, 0);
    let normal = textureLoad(normals, p, 0);
    if normal.a == 0.0 || surface.a >= camera.settings.z || camera.settings.w != 0.0 { discard; }
    let depth = textureLoad(opaque_depth, p, 0).r;
    // Both snapshots reduce the same forward samples. Any nearer water masks the receiver.
    if abs(textureLoad(final_depth, p, 0).r-depth) > max(0.001, depth*0.0001) { discard; }
    let uv = pixel.xy / camera.size.xy;
    let lod = clamp(surface.a / camera.settings.z, 0.0, 1.0) * f32(REFLECTION_LEVELS);
    let lower = u32(floor(lod));
    let upper = min(lower+1u, REFLECTION_LEVELSu);
    let reflection = mix(reflected(lower, uv, sample), reflected(upper, uv, sample), fract(lod));
    if reflection.a <= 0.0 { discard; }
    let cosine = clamp(dot(reflection_normal(normal.xyz), -reflection_incident(uv)), 0.0, 1.0);
    let fresnel = surface.rgb + (vec3<f32>(1.0)-surface.rgb)*pow(1.0-cosine,5.0);
    let roughness_fade = 1.0 - smoothstep(camera.limits.x, camera.settings.z, surface.a);
    var factor = reflection_factor(fresnel, vec2<f32>(1.0, 0.0));
    var environment = vec3<f32>(0.0);
    let replacement = camera.size.w != 0.0;
    if replacement {
        factor = textureLoad(forward_factor, p, FORWARD_SAMPLE).rgb;
        environment = textureLoad(forward_environment, p, FORWARD_SAMPLE).rgb;
    }
    return vec4<f32>(reflection_delta(reflection.rgb, reflection.a, factor, environment, replacement) * roughness_fade, 0.0);
}
fn reflection_coordinates(uv: vec2<f32>, size: vec2<i32>, footprint: f32) -> mat3x2<f32> {
    let base = vec2<f32>(textureDimensions(reflection_0)) / vec2<f32>(SAMPLE_LANES, 1);
    let position = uv * base;
    let lower = clamp(floor(position / footprint - 0.5), vec2<f32>(0.0), vec2<f32>(size - 1));
    let upper = min(lower + 1.0, vec2<f32>(size - 1));
    // A partial final bin is centred on its actual footprint, not a full-width bin.
    let a = min((lower + 0.5) * footprint, (lower * footprint + base) * 0.5);
    let b = min((upper + 0.5) * footprint, (upper * footprint + base) * 0.5);
    let weight = clamp((position - a) / max(b - a, vec2<f32>(0.5)), vec2<f32>(0.0), vec2<f32>(1.0));
    return mat3x2<f32>(lower, upper, weight);
}
