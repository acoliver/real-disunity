struct ReflectionSum {
    pair: vec4<f32>,
    weight: f32,
}
fn reflection_raw(pixel: vec2<i32>, sample: u32) -> vec4<f32> {
    let offset = vec2<i32>(i32(sample) * reflection_extent(0u).x, 0);
    return textureLoad(reflection_0, pixel + offset, 0);
}
fn reflection_normalized(sum: ReflectionSum) -> vec4<f32> {
    if sum.weight == 0.0 { return vec4<f32>(0.0); }
    return sum.pair / sum.weight;
}
fn reflection_tent(uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver, lod: f32) -> vec4<f32> {
    let centre = uv * vec2<f32>(reflection_extent(0u)) - 0.5;
    let radius = clamp(2.0 * exp2(lod), 2.0, 4.0);
    let first = max(vec2<i32>(ceil(centre - radius)), vec2<i32>(0));
    let last = min(vec2<i32>(floor(centre + radius)), reflection_extent(0u) - 1);
    var sum = ReflectionSum(vec4<f32>(0.0), 0.0);
    for (var y = first.y; y <= last.y; y++) {
        for (var x = first.x; x <= last.x; x++) {
            let p = vec2<i32>(x, y);
            let tent = max(vec2<f32>(radius) - abs(vec2<f32>(p) - centre), vec2<f32>(0.0));
            let weight = tent.x * tent.y;
            if weight > 0.0 && reflection_compatible(receiver, reflection_trace_receiver(p, sample)) {
                sum.pair += reflection_raw(p, sample) * weight;
                sum.weight += weight;
            }
        }
    }
    return reflection_normalized(sum);
}
fn reflection_level(level: u32, uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver) -> vec4<f32> {
    if level > 0u { return reflected(level, uv, sample); }
    let coordinates = reflection_coordinates(uv, reflection_extent(0u), 1.0);
    let a = vec2<i32>(coordinates[0]);
    let b = vec2<i32>(coordinates[1]);
    let w = coordinates[2];
    let pixels = array<vec2<i32>, 4>(a, vec2<i32>(b.x, a.y), vec2<i32>(a.x, b.y), b);
    let weights = vec4<f32>((1.0-w.x)*(1.0-w.y), w.x*(1.0-w.y), (1.0-w.x)*w.y, w.x*w.y);
    var sum = ReflectionSum(vec4<f32>(0.0), 0.0);
    for (var i = 0u; i < 4u; i++) {
        if weights[i] > 0.0 && reflection_compatible(receiver, reflection_trace_receiver(pixels[i], sample)) {
            sum.pair += reflection_raw(pixels[i], sample) * weights[i];
            sum.weight += weights[i];
        }
    }
    return reflection_normalized(sum);
}
fn reconstructed_reflection(uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver, roughness: f32) -> vec4<f32> {
    if roughness == 0.0 { return reflection_level(0u, uv, sample, receiver); }
    let lod = reflection_lod(roughness);
    if lod <= 1.0 { return reflection_tent(uv, sample, receiver, lod); }
    let lower = u32(floor(lod));
    let upper = min(lower + 1u, REFLECTION_LEVELSu);
    let a = reflection_level(lower, uv, sample, receiver);
    var broad = a;
    if fract(lod) > 0.0 { broad = mix(a, reflection_level(upper, uv, sample, receiver), fract(lod)); }
    if lod >= 2.0 { return broad; }
    // Smooth support transition; only the low-LOD tent has a four-texel radius.
    return mix(reflection_tent(uv, sample, receiver, 1.0), broad, smoothstep(1.0, 2.0, lod));
}
