@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var occlusion: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(position.xy);
    let size = vec2<i32>(textureDimensions(depths));
    let centre = textureLoad(depths, pixel, 0).r;
    var sum = 0.0;
    var weights = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let p = pixel + vec2<i32>(x, y);
            if any(p < vec2<i32>(0)) || any(p >= size) { continue; }
            let difference = abs(textureLoad(depths, p, 0).r - centre);
            let threshold = max(0.001, screen.settings.x * 0.05);
            if difference > threshold { continue; }
            let weight = f32((3 - abs(x)) * (3 - abs(y))) * exp(-difference / threshold);
            sum += textureLoad(occlusion, p, 0).r * weight;
            weights += weight;
        }
    }
    return sum / weights;
}
