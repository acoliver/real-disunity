fn water_screen_tap_depth(pixel: vec2<i32>, fraction: vec2<f32>, size: vec2<i32>) -> vec2<f32> {
    let ndc = (vec2<f32>(pixel) + fraction) / vec2<f32>(size) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    let near_h = water.inverse_clip * vec4<f32>(ndc, 0.0, 1.0);
    let far_h = water.inverse_clip * vec4<f32>(ndc, 1.0, 1.0);
    let start = near_h.xyz / near_h.w;
    let ray = far_h.xyz / far_h.w - start;
    let intersection = (water.origin_time.z - start.z) / ray.z;
    let depth = -(water.view * vec4<f32>(start + ray * intersection, 1.0)).z;
    return vec2<f32>(intersection, depth);
}

fn water_screen_background(original: vec2<i32>, offset: vec2<f32>, fraction: vec2<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(opaque_color));
    let coordinate = clamp(vec2<f32>(original) + offset * vec2<f32>(size), vec2<f32>(0.0), vec2<f32>(size - vec2<i32>(1)));
    let base = vec2<i32>(floor(coordinate));
    let blend = fract(coordinate);
    var color = vec3<f32>(0.0);
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let weight = select(1.0 - blend.x, blend.x, x == 1) * select(1.0 - blend.y, blend.y, y == 1);
            if weight == 0.0 { continue; }
            let pixel = min(base + vec2<i32>(x, y), size - vec2<i32>(1));
            let surface = water_screen_tap_depth(pixel, fraction, size);
            if surface.x <= 0.0 || textureLoad(opaque_depth, pixel, 0).r < surface.y {
                return textureLoad(opaque_color, original, 0).rgb;
            }
            color += textureLoad(opaque_color, pixel, 0).rgb * weight;
        }
    }
    return color;
}
