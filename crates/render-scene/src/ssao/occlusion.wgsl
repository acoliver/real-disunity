@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var normals: texture_2d<f32>;
fn view_position(pixel: vec2<i32>) -> vec3<f32> {
    let ray = reconstruct(vec2<f32>(pixel) + vec2<f32>(0.5), textureDimensions(depths), 0.0);
    return ray * (-textureLoad(depths, pixel, 0).r / ray.z);
}
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(position.xy);
    let packed = textureLoad(normals, pixel, 0);
    if packed.a == 0.0 { return 1.0; }
    let normal = normalize(packed.xyz);
    let centre = view_position(pixel);
    let tangent = normalize(cross(select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(normal.z) > 0.9), normal));
    let bitangent = cross(normal, tangent);
    let count = u32(screen.samples.x);
    var occluded = 0.0;
    for (var i = 0u; i < count; i++) {
        let t = (f32(i) + 0.5) / f32(count);
        let angle = f32(i) * 2.39996323;
        let z = t;
        let r = sqrt(1.0 - z * z);
        let direction = tangent * (cos(angle) * r) + bitangent * (sin(angle) * r) + normal * z;
        if dot(direction, normal) < screen.settings.w { continue; }
        let point = centre + direction * screen.settings.x * (0.2 + 0.8 * t);
        let clip = screen.projection * vec4<f32>(point, 1.0);
        if clip.w <= 0.0 { continue; }
        let uv = clip.xy / clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
        if any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0)) { continue; }
        let p = vec2<i32>(uv * vec2<f32>(textureDimensions(depths)));
        let sample_depth = textureLoad(depths, p, 0).r;
        let distance = abs(sample_depth + centre.z);
        if sample_depth < -point.z - screen.settings.z && distance < screen.settings.x {
            occluded += 1.0 - distance / screen.settings.x;
        }
    }
    return clamp(1.0 - screen.settings.y * occluded / f32(count), 0.0, 1.0);
}
