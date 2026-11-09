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
// Each reduction averages its valid children equally. Partial ceil bins are
// therefore NOT area averages; floor levels can discard whole child subtrees.
fn reflection_leaf_weight(pixel: vec2<i32>, level: u32) -> f32 {
    var child = pixel;
    var weight = 1.0;
    for (var step = 1u; step <= level; step++) {
        let parent = child / 2;
        if any(parent >= reflection_extent(step)) { return 0.0; }
        let count = min(vec2<i32>(2), reflection_extent(step - 1u) - parent * 2);
        weight /= f32(count.x * count.y);
        child = parent;
    }
    return weight;
}
fn reflection_bin_compatible(bin: vec2<i32>, level: u32, sample: u32, receiver: ReflectionReceiver) -> bool {
    let span = i32(1u << level);
    let first = bin * span;
    let end = min(first + span, reflection_extent(0u));
    for (var y = first.y; y < end.y; y++) {
        for (var x = first.x; x < end.x; x++) {
            let p = vec2<i32>(x, y);
            if reflection_leaf_weight(p, level) > 0.0 && !reflection_compatible(receiver, reflection_trace_receiver(p, sample)) {
                return false;
            }
        }
    }
    return true;
}
fn reflection_bin_sum(bin: vec2<i32>, level: u32, sample: u32, receiver: ReflectionReceiver) -> ReflectionSum {
    let span = i32(1u << level);
    let first = bin * span;
    let end = min(first + span, reflection_extent(0u));
    var sum = ReflectionSum(vec4<f32>(0.0), 0.0);
    for (var y = first.y; y < end.y; y++) {
        for (var x = first.x; x < end.x; x++) {
            let p = vec2<i32>(x, y);
            let weight = reflection_leaf_weight(p, level);
            if weight > 0.0 && reflection_compatible(receiver, reflection_trace_receiver(p, sample)) {
                sum.pair += reflection_raw(p, sample) * weight;
                sum.weight += weight;
            }
        }
    }
    return sum;
}
fn reflection_level(level: u32, uv: vec2<f32>, sample: u32, receiver: ReflectionReceiver) -> vec4<f32> {
    let coordinates = reflection_coordinates(uv, reflection_extent(level), exp2(f32(level)));
    let a = vec2<i32>(coordinates[0]);
    let b = vec2<i32>(coordinates[1]);
    let w = coordinates[2];
    let bins = array<vec2<i32>, 4>(a, vec2<i32>(b.x, a.y), vec2<i32>(a.x, b.y), b);
    let weights = vec4<f32>((1.0-w.x)*(1.0-w.y), w.x*(1.0-w.y), (1.0-w.x)*w.y, w.x*w.y);
    var compatible = true;
    for (var i = 0u; i < 4u; i++) {
        if weights[i] > 0.0 && !reflection_bin_compatible(bins[i], level, sample, receiver) {
            compatible = false;
            break;
        }
    }
    // Verify metadata only, stopping at the first rejection. Homogeneous support
    // uses the actual premip sample, including reduction storage rounding.
    if compatible { return reflected(level, uv, sample); }
    var sum = ReflectionSum(vec4<f32>(0.0), 0.0);
    for (var i = 0u; i < 4u; i++) {
        if weights[i] > 0.0 {
            let bin = reflection_bin_sum(bins[i], level, sample, receiver);
            sum.pair += bin.pair * weights[i];
            sum.weight += bin.weight * weights[i];
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
