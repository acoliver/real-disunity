// Reflection consumers normalize once at entry; shading and metadata retain fold magnitude.
fn reflection_normal(normal: vec3<f32>) -> vec3<f32> {
    return normalize(normal);
}
