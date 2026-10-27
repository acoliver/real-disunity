fn water_noise_permutation(value: f32) -> f32 {
    let polynomial = value * (34.0 * value + 1.0);
    return polynomial - floor(polynomial / 289.0) * 289.0;
}
fn water_noise_corner(cell: vec2<f32>, delta: vec2<f32>) -> f32 {
    let wrapped = cell - floor(cell / 289.0) * 289.0;
    let hash = water_noise_permutation(water_noise_permutation(wrapped.y) + wrapped.x);
    let horizontal = 2.0 * fract(hash / 41.0) - 1.0;
    let gradient = vec2<f32>(horizontal - floor(horizontal + 0.5), abs(horizontal) - 0.5);
    let attenuation = max(0.5 - dot(delta, delta), 0.0);
    let normalization = 1.79284291400159 - 0.85373472095314 * dot(gradient, gradient);
    return attenuation * attenuation * attenuation * attenuation * normalization * dot(gradient, delta);
}
fn water_edge_noise(point: vec2<f32>) -> f32 {
    let skew = 0.366025403784439;
    let unskew = 0.211324865405187;
    let cell = floor(point + dot(point, vec2<f32>(skew)));
    let delta = point - cell + dot(cell, vec2<f32>(unskew));
    let step = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), delta.x > delta.y);
    return 130.0 * (water_noise_corner(cell, delta)
        + water_noise_corner(cell + step, delta - step + unskew)
        + water_noise_corner(cell + 1.0, delta - 1.0 + 2.0 * unskew));
}
fn water_environment_gain(point: vec2<f32>, depth: f32) -> f32 {
    var gain = water.environment_terms.x;
    if water.environment_terms.y != 0.0 {
        let noise = min((water_edge_noise(point * water.environment_terms.y) + 1.0) * 0.5, 0.6);
        let edge = 1.0 - clamp(pow(clamp(depth, 0.0, 1.0), 0.2) * 2.0 - 0.5, 0.0, 1.0);
        gain *= 1.0 - noise * edge;
    }
    if water.environment_terms.z != 0.0 { gain *= clamp(depth * 20.0, 0.0, 1.0); }
    return gain;
}

fn water_disk_direction(reflected: vec3<f32>, center: vec3<f32>) -> vec3<f32> {
    let cosine = dot(reflected, center);
    let radius_cosine = cos(0.0027);
    if cosine >= radius_cosine { return reflected; }
    let radial = reflected - cosine * center;
    var tangent = cross(center, select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(center.x) > 0.9));
    if dot(radial, radial) > 0.0 { tangent = radial; }
    return normalize(center * radius_cosine + normalize(tangent) * sin(0.0027));
}
fn water_disk_highlight(normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, f0: vec3<f32>, roughness: f32, visibility: f32) -> vec3<f32> {
    let selected = water_disk_direction(reflect(-view, normal), light);
    let halfway = normalize(view + selected);
    let dots = vec4<f32>(clamp(dot(normal, view), 0.0001, 1.0),
        max(dot(normal, selected), 0.0), max(dot(normal, halfway), 0.0), max(dot(view, halfway), 0.0));
    let fresnel = fresnel_schlick(dots.w, f0);
    var alpha = max(roughness, 0.04);
    if SCENE_ROUGHNESS_REMAP != 0u { alpha = max(scene_environment_roughness(roughness), 0.04); }
    let highlight = scene_specular_brdf(dots, alpha, fresnel)
        * water.sun_radiance.xyz * max(dot(normal, light), 0.0) * water.disk.x * visibility;
    let luminance = dot(highlight, vec3<f32>(0.2126, 0.7152, 0.0722));
    if luminance <= water.disk.y { return highlight; }
    return highlight * (water.disk.y / luminance);
}

fn water_pack_normal(normal: vec3<f32>) -> vec2<f32> {
    let projected = normal / (abs(normal.x) + abs(normal.y) + abs(normal.z));
    if projected.z >= 0.0 { return projected.xy; }
    let signs = select(vec2<f32>(-1.0), vec2<f32>(1.0), projected.xy >= vec2<f32>(0.0));
    return (vec2<f32>(1.0) - abs(projected.yx)) * signs;
}

fn water_wind_scale(position: vec3<f32>, time: u32, band: vec3<f32>) -> f32 {
    let cycle = 6.283185307179586;
    let period = u32(cycle / band.x * 1000.0);
    let clock = f32(time % period) / 1000.0;
    let spatial = dot(position, vec3<f32>(band.z));
    let wrapped = spatial - floor(spatial / cycle) * cycle;
    let phase = clock * band.x + wrapped;
    let reduced = phase - floor((phase + 3.141592653589793) / cycle) * cycle;
    return 1.0 + sin(clamp(reduced, -3.141592653589793, 3.141592653589793)) * band.y;
}
fn water_wind_seed(seed: vec2<f32>, force: vec2<f32>) -> vec2<f32> {
    let direction = force / max(0.001, length(force));
    let strength = abs((force.x + force.y) * 0.25);
    return seed * clamp(strength, 0.2, 1.0) * direction * 2.0 - direction * strength * 2.0;
}
