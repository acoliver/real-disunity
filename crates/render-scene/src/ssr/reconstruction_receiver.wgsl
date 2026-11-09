struct ReflectionReceiver {
    position: vec3<f32>,
    normal: vec3<f32>,
    owner: u32,
}
fn reflection_receiver_visible(p: vec2<i32>, depth: f32) -> bool {
    return abs(textureLoad(final_depth, p, 0).r-depth) <= max(0.001, depth*0.0001);
}
fn reflection_receiver(p: vec2<i32>, uv: vec2<f32>, sample: u32) -> ReflectionReceiver {
    // WATER_RECONSTRUCTION_RECEIVER
    let packed = textureLoad(normals, p, 0);
    let depth = textureLoad(opaque_depth, p, 0).r;
    if packed.a == 0.0 || depth <= 0.0 || !reflection_receiver_visible(p, depth) {
        return ReflectionReceiver(vec3<f32>(0.0), vec3<f32>(0.0), 0u);
    }
    return ReflectionReceiver(reflection_position(uv, depth), reflection_normal(packed.xyz), 1u);
}
fn reflection_trace_receiver(pixel: vec2<i32>, sample: u32) -> ReflectionReceiver {
    let uv = (vec2<f32>(pixel) + 0.5) / vec2<f32>(reflection_extent(0u));
    let p = min(vec2<i32>(uv * camera.size.xy), vec2<i32>(camera.size.xy) - 1);
    return reflection_receiver(p, uv, sample);
}
fn reflection_compatible(a: ReflectionReceiver, b: ReflectionReceiver) -> bool {
    if a.owner == 0u || a.owner != b.owner || dot(a.normal, b.normal) < 0.9 { return false; }
    let delta = b.position - a.position;
    // Symmetric tangent-plane distance admits slopes without admitting depth steps.
    let residual = max(abs(dot(delta, a.normal)), abs(dot(delta, b.normal)));
    return residual <= max(0.01, min(-a.position.z, -b.position.z) * 0.001);
}
