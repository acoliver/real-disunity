fn water_shadow_response(raw: f32, depth_weight: f32) -> f32 {
    if water.disk.w == 0.0 { return raw; }
    let floored = max(raw, water.wind_flags.z);
    if (u32(water.wind_flags.w) & 1u) != 0u { return mix(floored, 1.0, depth_weight); }
    return floored;
}
fn water_shadow_disk(limited: vec3<f32>, raw: f32) -> vec3<f32> {
    if water.disk.w != 0.0 && (u32(water.wind_flags.w) & 2u) != 0u { return limited * raw; }
    return limited;
}
