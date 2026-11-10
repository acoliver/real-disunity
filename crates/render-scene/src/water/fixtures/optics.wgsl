fn water_flow_at_depth(depth: f32) -> f32 {
    return pow(clamp(depth, 0.0, 1.0), 0.2) * 0.2;
}
fn water_normal_at_depth(normal: vec3<f32>, depth: f32) -> vec3<f32> {
    return vec3<f32>(abs(normal.xy) * clamp(pow(max(depth, 0.0), 0.4) * 2.0, 0.0, 1.0), normal.z);
}
fn water_opacity_at_depth(depth: f32) -> f32 {
    return clamp(depth * 20.0, 0.0, 1.0);
}
fn water_refraction_offset(normal: vec2<f32>, depth: f32, bound: f32) -> vec2<f32> {
    return normal * bound * clamp(depth, 0.0, 1.0);
}
fn water_view_scattering(view: vec3<f32>, radiance: vec3<f32>) -> vec3<f32> {
    let limiter = clamp(min(pow(1.0 - view.z, 2.0), 0.5), 0.0, 1.0);
    return radiance * clamp(1.0 - limiter, 0.0, 1.0);
}

fn water_tint_weights(depth: f32, controls: vec4<f32>) -> vec2<f32> {
    let ramps = clamp(vec2<f32>(1.0) / pow(vec2<f32>(100.0), controls.xy * max(depth, 0.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    return vec2<f32>(clamp(clamp(ramps.x - controls.z, 0.0, 1.0) + controls.w, 0.0, 1.0), ramps.y);
}
fn water_tint_color(background: vec3<f32>, depth: f32, deep: vec3<f32>, shallow: vec3<f32>, controls: vec4<f32>) -> vec3<f32> {
    let weights = water_tint_weights(depth, controls);
    let first = mix(pow(deep, vec3<f32>(2.2)), background * 4.0 * deep, weights.x);
    let second = mix(pow(shallow, vec3<f32>(2.2)), background * 4.0 * shallow, weights.x);
    return mix(first, second, weights.y) * 0.25;
}
