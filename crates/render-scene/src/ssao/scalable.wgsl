// McGuire, Mara and Luebke, Scalable Ambient Obscurance (HPG 2012).
// https://research.nvidia.com/publication/2012-06_scalable-ambient-obscurance
// Separate depth footprints preserve the shared conservative depth hierarchy.
/* DEPTH_BINDINGS */
fn filtered_depth(image: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(image));
    let coordinate = uv * vec2<f32>(size) - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coordinate));
    let fraction = fract(coordinate);
    let a = textureLoad(image, clamp(base, vec2<i32>(0), size - 1), 0).r;
    let b = textureLoad(image, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), size - 1), 0).r;
    let c = textureLoad(image, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), size - 1), 0).r;
    let d = textureLoad(image, clamp(base + vec2<i32>(1), vec2<i32>(0), size - 1), 0).r;
    return mix(mix(a, b, fraction.x), mix(c, d, fraction.x), fraction.y);
}
/* DEPTH_SELECT */
fn view_point(pixel: vec2<f32>, size: vec2<u32>, depth: f32) -> vec3<f32> {
    let clip = screen.projection * vec4<f32>(0.0, 0.0, -depth, 1.0);
    return reconstruct(pixel, size, clip.z / clip.w);
}
fn phase(pixel: vec2<i32>) -> f32 {
    let a = (pixel.x & 65535) * ((pixel.y & 255) + 1);
    let b = (pixel.x >> 16) + (((pixel.x + 127) & 65535) * ((pixel.y + 128) & 255));
    return fract(f32(a ^ b) * 2.39996);
}
fn tap_offset(index: u32, count: u32, rotation: f32, radius: f32) -> vec2<f32> {
    let t = (f32(index) + rotation + 0.5) * (1.0 / f32(count));
    let squared = t * t;
    let angle = squared * (screen.samples.y * 6.2831853) + rotation * 6.2831853;
    return radius * squared * vec2<f32>(cos(angle), sin(angle));
}
fn tap_level(radius: f32) -> i32 {
    return clamp(i32(floor(log2(max(radius, 0.00000001)))) - 3, 0, MAX_LEVEL);
}
fn projected_radius(centre: vec3<f32>, height: u32) -> f32 {
    let projected = screen.projection * vec4<f32>(centre, 1.0);
    return abs(screen.projection[1][1]) * f32(height) * 0.5 * screen.settings.x / projected.w;
}
fn obscurance(delta: vec3<f32>, normal: vec3<f32>) -> f32 {
    let squared_distance = dot(delta, delta);
    let falloff = clamp(1.0 - squared_distance / (screen.settings.x * screen.settings.x), 0.0, 1.0);
    let contribution = falloff * clamp(dot(delta, normal) - screen.settings.z, 0.0, 1.0) / (0.0002 + squared_distance);
    let strength = screen.settings.y;
    let blend = clamp(1.1 - strength * 0.15, 0.0, 1.0);
    return contribution * mix(contribution, 0.9 + 0.5 * strength, blend);
}
fn visibility(accumulation: f32, count: u32) -> f32 {
    let strength_scale = min(1.0, screen.settings.y * 1.5);
    return pow(clamp(1.0 - sqrt(accumulation * strength_scale / f32(count)), 0.0, 1.0), screen.samples.z);
}
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) vec2<f32> {
    let pixel = vec2<i32>(position.xy);
    let size = textureDimensions(footprint0);
    let full_pixel = min(pixel * 2 + 1, vec2<i32>(textureDimensions(surface_normals)) - 1);
    let packed_normal = textureLoad(surface_normals, full_pixel, 0);
    if packed_normal.a == 0.0 { return vec2<f32>(1.0, 1.0); }
    let depth = textureLoad(footprint0, pixel, 0).r;
    let centre = view_point(position.xy, size, depth);
    let normal = normalize(packed_normal.xyz);
    let radius = projected_radius(centre, size.y);
    let rotation = phase(pixel);
    let count = u32(screen.samples.x);
    var accumulation = 0.0;
    for (var index = 0u; index < count; index++) {
        let offset = tap_offset(index, count, rotation, radius);
        let level = tap_level(length(offset));
        let endpoint = position.xy + offset;
        let uv = min(endpoint / vec2<f32>(size), vec2<f32>(1.0) - vec2<f32>(f32(1u << u32(level + 1))) / vec2<f32>(size));
        let sample_depth = depth_at(uv, level);
        let delta = view_point(endpoint, size, sample_depth) - centre;
        accumulation += obscurance(delta, normal);
    }
    let far_point = reconstruct(vec2<f32>(0.5), vec2<u32>(1), 1.0);
    return vec2<f32>(visibility(accumulation, count), clamp(depth / -far_point.z, 0.0, 1.0));
}
