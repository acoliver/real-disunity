fn reflection_near(uv: vec2<f32>) -> vec3<f32> {
    let h = camera.inverse_projection * vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return h.xyz / h.w;
}
fn reflection_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let near = reflection_near(uv);
    if camera.projection[3][3] != 0.0 { return vec3<f32>(near.xy, -depth); }
    return near * (-depth / near.z);
}
fn reflection_incident(uv: vec2<f32>) -> vec3<f32> {
    if camera.projection[3][3] != 0.0 { return vec3<f32>(0.0, 0.0, -1.0); }
    return normalize(reflection_near(uv));
}
