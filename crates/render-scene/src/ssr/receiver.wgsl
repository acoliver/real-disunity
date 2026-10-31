fn gathered_visibility(pixel: vec2<i32>, cosine: f32, roughness: f32, sample: u32) -> f32 {
    let uv = (vec2<f32>(pixel) + vec2<f32>(0.5)) / camera.size.xy;
    let depth = textureLoad(opaque_depth, pixel, 0).r;
    let ceiling = textureLoad(forward_factor, pixel, FORWARD_SAMPLE).a;
    let gathered = min(1.01 * receiver_gather(uv, depth), ceiling);
    return rd_specular_occlusion(cosine, gathered, roughness);
}
