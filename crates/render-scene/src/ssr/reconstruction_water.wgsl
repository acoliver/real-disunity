fn reflection_water_receiver(p: vec2<i32>, sample: u32, packed: vec4<f32>) -> ReflectionReceiver {
    let surface = textureLoad(water_material, p, SAMPLE_INDEX);
    let uv = (vec2<f32>(p) + surface.rg) / camera.size.xy;
    let raw = textureLoad(water_depth, p, SAMPLE_INDEX);
    let h = camera.inverse_projection * vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), raw, 1.0);
    return ReflectionReceiver(h.xyz / h.w, reflection_normal(water_receiver_normal(packed).xyz), vec3<f32>(0.0), 2u);
}
fn reflection_receiver_visible(p: vec2<i32>, depth: f32) -> bool {
    // Only water writes depth after the opaque snapshot. Reject any water-covered lane.
    // This is more conservative than main when water covers a far opaque lane
    // while a nearer opaque lane sets the pixel's minimum depth.
    // Raw unprojection and the opaque snapshot compute depth differently and
    // disagree after float32 rounding, so comparing them rejects visible land.
    for (var sample = 0u; sample < SAMPLE_LANESu; sample++) {
        if textureLoad(water_normals, p, SAMPLE_INDEX).a != 0.0 { return false; }
    }
    return true;
}
