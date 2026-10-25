// Incident RGB is confidence-premultiplied linear HDR on the same radiometric
// scale as the published forward lobe. The dimensionless factor includes BRDF
// and camera transmittance, never exposure already present in incident radiance.
fn reflection_factor(f0: vec3<f32>, coefficients: vec2<f32>) -> vec3<f32> {
    return f0 * coefficients.x + vec3<f32>(coefficients.y);
}
fn reflection_delta(incident: vec3<f32>, confidence: f32, factor: vec3<f32>, environment: vec3<f32>, replacement: bool) -> vec3<f32> {
    return incident * factor - select(vec3<f32>(0.0), environment * confidence, replacement);
}
