struct ReflectionCamera {
    projection: mat4x4<f32>, inverse_projection: mat4x4<f32>,
    settings: vec4<f32>, size: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: ReflectionCamera;
@group(0) @binding(1) var opaque: texture_2d<f32>;
@group(0) @binding(2) var normals: texture_2d<f32>;
@group(0) @binding(3) var materials: texture_2d<f32>;
@group(0) @binding(4) var depth_0: texture_2d<f32>;
@group(0) @binding(5) var depth_2: texture_2d<f32>;
@group(0) @binding(6) var depth_4: texture_2d<f32>;
@group(0) @binding(7) var depth_6: texture_2d<f32>;
fn depth_at(level: u32, uv: vec2<f32>) -> f32 {
    let scale = exp2(min(f32(level*2u), ceil(log2(max(camera.size.x,camera.size.y)))));
    let coordinate = vec2<i32>(uv*camera.size.xy/scale);
    switch level {
        case 0u: { return textureLoad(depth_0, coordinate, 0).r; }
        case 1u: { return textureLoad(depth_2, coordinate, 0).r; }
        case 2u: { return textureLoad(depth_4, coordinate, 0).r; }
        default: { return textureLoad(depth_6, coordinate, 0).r; }
    }
}
fn edge_fade(uv: vec2<f32>) -> f32 {
    let edge = min(min(uv.x, uv.y), min(1.0-uv.x, 1.0-uv.y));
    return smoothstep(0.0, 0.08, edge);
}
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Trace targets can be half resolution, with ceil-divided odd extents.
    let trace_size = ceil(camera.size.xy / camera.size.z);
    let origin_uv = pixel.xy / trace_size;
    let p = clamp(vec2<i32>(origin_uv * camera.size.xy), vec2<i32>(0), vec2<i32>(camera.size.xy)-vec2<i32>(1));
    let packed = textureLoad(normals, p, 0);
    let roughness = textureLoad(materials, p, 0).a;
    if packed.a == 0.0 || roughness >= camera.settings.z || camera.settings.w != 0.0 { return vec4<f32>(0.0); }
    let ray = camera.inverse_projection * vec4<f32>(origin_uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    let centre = ray.xyz * (-textureLoad(depth_0, p, 0).r / ray.z);
    let normal = normalize(packed.xyz);
    let direction = reflect(normalize(centre), normal);
    let start = centre + normal * 0.005;
    var distance = 100.0;
    if direction.z > 0.0 { distance = min(distance, max(0.0, (-0.01-start.z)/direction.z)); }
    let end = start + direction * distance;
    let a = camera.projection * vec4<f32>(start, 1.0);
    let b = camera.projection * vec4<f32>(end, 1.0);
    if a.w <= 0.0 || b.w <= 0.0 { return vec4<f32>(0.0); }
    let uv_a = a.xy/a.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let uv_b = b.xy/b.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let span = length((uv_b-uv_a) * camera.size.xy);
    if span < 2.0 { return vec4<f32>(0.0); }
    let stride = 1.0/span;
    var t = 2.0 * stride;
    var level = 0u;
    for (var step = 0u; step < u32(camera.settings.x); step++) {
        if t >= 1.0 { break; }
        let sample_uv = mix(uv_a, uv_b, t);
        if any(sample_uv <= vec2<f32>(0.0)) || any(sample_uv >= vec2<f32>(1.0)) { break; }
        let point = mix(start/a.w, end/b.w, t) / mix(1.0/a.w, 1.0/b.w, t);
        let scale = exp2(min(f32(level*2u), ceil(log2(max(camera.size.x,camera.size.y)))));
        let cell = floor(sample_uv*camera.size.xy/scale);
        let delta = (uv_b-uv_a)*camera.size.xy;
        let boundary = (cell + select(vec2<f32>(0.0), vec2<f32>(1.0), delta > vec2<f32>(0.0))) * scale;
        let crossing = (boundary - uv_a*camera.size.xy) / select(delta, vec2<f32>(1e-20), abs(delta) < vec2<f32>(1e-20));
        let exit_t = min(min(select(1e20, crossing.x, crossing.x > t+1e-7), select(1e20, crossing.y, crossing.y > t+1e-7)), 1.0);
        let next_t = max(t+stride*0.001, exit_t);
        let next_point = mix(start/a.w, end/b.w, next_t) / mix(1.0/a.w, 1.0/b.w, next_t);
        let surface_depth = depth_at(level, sample_uv);
        let difference = -point.z - surface_depth;
        let next_difference = -next_point.z - surface_depth;
        if max(difference,next_difference) >= 0.0 {
            if level > 0u { level--; continue; }
            // The first crossed pixel terminates traversal even if too thick to accept.
            if difference > camera.settings.y { break; }
            let hit_uv = sample_uv;
            let hit_pixel = vec2<i32>(hit_uv * camera.size.xy);
            let hit_normal = textureLoad(normals, hit_pixel, 0);
            if hit_normal.a == 0.0 || dot(hit_normal.xyz, direction) >= 0.0 { break; }
            let hit = textureLoad(opaque, hit_pixel, 0).rgb;
            let budget_fade = smoothstep(0.0, 0.2, 1.0-f32(step)/camera.settings.x);
            let thickness_fade = 1.0-smoothstep(camera.settings.y*0.5, camera.settings.y, max(difference,0.0));
            let confidence = edge_fade(hit_uv) * edge_fade(origin_uv) * budget_fade * thickness_fade;
            return vec4<f32>(hit * confidence, confidence);
        }
        t = next_t + stride*0.001;
        level = min(level+1u, 3u);
    }
    return vec4<f32>(0.0);
}
