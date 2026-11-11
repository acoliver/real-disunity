fn water_shadow_normal(normal: vec3<f32>) -> vec3<f32> {
    if water.disk.w == 2.0 { return vec3<f32>(0.0, 0.0, 1.0); }
    if water.disk.w == 3.0 { return vec3<f32>(0.0); }
    return normal;
}
fn water_shadow_depth(depth: f32) -> f32 {
    if water.tint_deep.w != 0.0 { return water_tint_weights(depth, water.tint_controls).x; }
    return clamp(depth, 0.0, 1.0);
}
