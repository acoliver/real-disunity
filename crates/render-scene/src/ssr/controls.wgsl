fn reflection_thickness(base: f32, slope: f32, depth: f32) -> f32 {
    return base + depth * slope;
}
fn bounded_radiance(radiance: vec3<f32>, ceiling: f32) -> vec3<f32> {
    let brightness = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
    return radiance * clamp(brightness, 0.0, ceiling) / max(0.0001, brightness);
}
