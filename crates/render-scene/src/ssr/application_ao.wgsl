fn reflection_far_depth() -> f32 {
    let point = camera.inverse_projection * vec4<f32>(0.0, 0.0, 1.0, 1.0);
    return -point.z / point.w;
}
fn gathered_visibility(pixel: vec2<i32>, cosine: f32, roughness: f32, sample: u32) -> f32 {
    let extent = vec2<i32>(textureDimensions(application_ao));
    let origin = vec2<i32>(floor((vec2<f32>(pixel) + 0.5) / camera.size.xy * vec2<f32>(extent) - 0.5));
    let a = clamp(origin, vec2<i32>(0), extent - 1);
    let b = clamp(origin + 1, vec2<i32>(0), extent - 1);
    let coordinates = array<vec2<i32>, 4>(vec2<i32>(a.x, b.y), b, vec2<i32>(b.x, a.y), a);
    var depths: vec4<f32>;
    var occlusion: vec4<f32>;
    for (var i = 0u; i < 4u; i++) {
        let tap = textureLoad(application_ao, coordinates[i], 0);
        depths[i] = APPLICATION_TAP_DEPTH;
        occlusion[i] = tap.r;
    }
    let ceiling = textureLoad(forward_factor, pixel, FORWARD_SAMPLE).a;
    let gathered = min(reflection_ao(depths, occlusion) * 1.01, ceiling);
    return rd_specular_occlusion(cosine, gathered, roughness);
}
