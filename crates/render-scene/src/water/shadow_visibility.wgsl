fn water_shadow_visibility(position: vec3<f32>, normal: vec3<f32>, shading_visibility: f32) -> f32 {
    // A zero normal bias makes every lookup identical. Reuse the shading lookup,
    // including its receiver-plane derivatives, rather than sampling it again.
    if water.disk.w < 2.0 || sunlight_normal_bias() == 0.0 { return shading_visibility; }
    return raw_sunlight_visibility(position, water_shadow_normal(normal));
}
