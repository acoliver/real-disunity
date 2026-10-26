fn reflection_thickness(base: f32, slope: f32, depth: f32) -> f32 {
    return base + depth * slope;
}
fn bounded_radiance(radiance: vec3<f32>, ceiling: f32) -> vec3<f32> {
    let brightness = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
    return radiance * clamp(brightness, 0.0, ceiling) / max(0.0001, brightness);
}

fn reflection_pass_count(roughness: f32, constant_count: f32, curve: vec4<f32>) -> u32 {
    if curve.x < 0.0 { return u32(constant_count); }
    let coefficient = clamp((roughness - curve.z) / (curve.w - curve.z), 0.0, 1.0);
    return u32(round(mix(curve.x, curve.y, coefficient)));
}
