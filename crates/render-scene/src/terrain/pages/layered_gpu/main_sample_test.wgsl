fn layered_sample(image: Image, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, repeat: bool) -> vec4<f32> {
    // Isotropic filtering uses the longest footprint basis vector. Supplying
    // equivalent axis-aligned gradients avoids backend-dependent rotated-ellipse
    // approximations while preserving the reference mip footprint.
    let size = vec2<f32>(image.shape.zw);
    let footprint = max(length(dx * size), length(dy * size));
    let horizontal = vec2<f32>(footprint / size.x, 0.0);
    let vertical = vec2<f32>(0.0, footprint / size.y);
    switch image.shape.x {
        case 0u: {
            if repeat { return textureSampleGrad(layered_array_0, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_0, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 1u: {
            if repeat { return textureSampleGrad(layered_array_1, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_1, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 2u: {
            if repeat { return textureSampleGrad(layered_array_2, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_2, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 3u: {
            if repeat { return textureSampleGrad(layered_array_3, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_3, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 4u: {
            if repeat { return textureSampleGrad(layered_array_4, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_4, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 5u: {
            if repeat { return textureSampleGrad(layered_array_5, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_5, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        case 6u: {
            if repeat { return textureSampleGrad(layered_array_6, layered_repeat, uv, i32(image.shape.y), horizontal, vertical); }
            return textureSampleLevel(layered_array_6, layered_clamp, uv, i32(image.shape.y), 0.0);
        }
        default: { return vec4<f32>(0.0); }
    }
}
