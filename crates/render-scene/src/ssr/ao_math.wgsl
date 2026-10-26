fn reflection_ao(depths: vec4<f32>, occlusion: vec4<f32>) -> f32 {
    let weights = exp(-abs(vec4<f32>(depths.x) - depths));
    return dot(weights, occlusion) / dot(weights, vec4<f32>(1.0));
}
