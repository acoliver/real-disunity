@group(0) @binding(1) var metadata0: texture_2d<f32>;
@group(0) @binding(2) var metadata1: texture_2d<f32>;
@group(0) @binding(3) var metadata2: texture_2d<f32>;
@group(0) @binding(4) var metadata3: texture_2d<f32>;
fn metadata(pixel: vec2<i32>, footprint: f32) -> vec4<f32> {
    if footprint >= 64.0 {
        return textureLoad(metadata3, min(pixel / LEVEL3_SCALE, vec2<i32>(textureDimensions(metadata3)) - vec2<i32>(1)), 0);
    }
    if footprint >= 16.0 {
        return textureLoad(metadata2, min(pixel / LEVEL2_SCALE, vec2<i32>(textureDimensions(metadata2)) - vec2<i32>(1)), 0);
    }
    if footprint >= 4.0 {
        return textureLoad(metadata1, min(pixel / LEVEL1_SCALE, vec2<i32>(textureDimensions(metadata1)) - vec2<i32>(1)), 0);
    }
    return textureLoad(metadata0, pixel, 0);
}
fn position_at(pixel: vec2<f32>, depth: f32) -> vec3<f32> {
    let projected = screen.projection * vec4<f32>(0.0, 0.0, -depth, 1.0);
    return reconstruct(pixel, textureDimensions(metadata0), projected.z / projected.w);
}
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(position.xy);
    let packed = textureLoad(metadata0, pixel, 0);
    if packed.w <= 0.0 { return 1.0; }
    let centre = position_at(position.xy, packed.w);
    let normal = normalize(packed.xyz);
    let size = vec2<f32>(textureDimensions(metadata0));
    let projected = screen.projection * vec4<f32>(centre, 1.0);
    let radius_pixels = abs(screen.projection[1][1] * screen.settings.x / projected.w) * size.y * 0.5;
    let count = u32(screen.samples.x);
    let rotation = fract(dot(vec2<f32>(pixel), vec2<f32>(0.06711056, 0.00583715))) * 6.2831853;
    var sum = 0.0;
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        let radius = max(1.0, radius_pixels * t * t);
        let angle = f32(i) * 2.39996323 + rotation;
        let p = pixel + vec2<i32>(round(vec2<f32>(cos(angle), sin(angle)) * radius));
        if any(p < vec2<i32>(0)) || any(p >= vec2<i32>(size)) { continue; }
        let sample = metadata(p, radius * 0.25);
        if sample.w <= 0.0 { continue; }
        let delta = position_at(vec2<f32>(p) + vec2<f32>(0.5), sample.w) - centre;
        let distance_squared = dot(delta, delta);
        if distance_squared <= 0.00000001 { continue; }
        let distance = sqrt(distance_squared);
        let cosine = dot(normal, delta) / distance;
        if cosine <= screen.settings.w { continue; }
        let falloff = max(0.0, 1.0 - distance_squared / (screen.settings.x * screen.settings.x));
        sum += falloff * max(0.0, cosine - screen.settings.z / distance);
    }
    return clamp(1.0 - screen.settings.y * sum / f32(count), 0.0, 1.0);
}
