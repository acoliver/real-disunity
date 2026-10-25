@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var lower: texture_2d<f32>;
struct Weights { taps: array<vec2<f32>, 33> }
@group(0) @binding(3) var<storage, read> weights: Weights;
override THRESHOLD: f32 = 1.0;
override KNEE: f32 = 0.5;
override TAPS: u32 = 0u;
override TINT_R: f32 = 1.0;
override TINT_G: f32 = 1.0;
override TINT_B: f32 = 1.0;
override WEIGHT: f32 = 1.0;
override FOLD: bool = false;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn clean(v: f32) -> f32 {
    let b = bitcast<u32>(v);
    if (b & 0x80000000u) != 0u || (b & 0x7f800000u) == 0x7f800000u { return 0.0; }
    return v;
}
fn load(p: vec2<i32>) -> vec3<f32> {
    return textureLoad(source, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(source)) - 1), 0).rgb;
}
fn bright(p: vec2<i32>) -> vec3<f32> {
    let c = load(p);
    let v = vec3<f32>(clean(c.r), clean(c.g), clean(c.b));
    let brightness = max(v.r, max(v.g, v.b));
    let knee = THRESHOLD * KNEE;
    var soft = clamp(brightness - THRESHOLD + knee, 0.0, 2.0 * knee);
    soft = soft * soft / max(4.0 * knee, 0.00001);
    return v * (max(brightness - THRESHOLD, soft) / max(brightness, 0.00001));
}
@fragment fn gaussian_bright(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let q = vec2<i32>(p.xy) * 2;
    return vec4<f32>((bright(q) + bright(q+vec2<i32>(1,0)) + bright(q+vec2<i32>(0,1)) + bright(q+vec2<i32>(1))) * 0.25, 1.0);
}
@fragment fn gaussian_downsample(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let q = vec2<i32>(p.xy) * 2;
    return vec4<f32>((load(q) + load(q+vec2<i32>(1,0)) + load(q+vec2<i32>(0,1)) + load(q+vec2<i32>(1))) * 0.25, 1.0);
}
fn sample_pair(uv: vec2<f32>, step: vec2<f32>, tap: vec2<f32>) -> vec3<f32> {
    return (textureSampleLevel(source, linear_sampler, uv + step * tap.x, 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv - step * tap.x, 0.0).rgb) * tap.y;
}
fn blur(uv: vec2<f32>, direction: vec2<f32>) -> vec3<f32> {
    let step = direction / vec2<f32>(textureDimensions(source));
    // Read taps directly from storage. Copying the array into a by-value parameter and
    // indexing it dynamically measured about two hundred times slower on one RDNA3 GPU.
    var value = textureSampleLevel(source, linear_sampler, uv, 0.0).rgb * weights.taps[0].y;
    for (var i = 1u; i <= TAPS; i++) { value += sample_pair(uv, step, weights.taps[i]); }
    return value;
}
@fragment fn gaussian_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(blur(p.xy / vec2<f32>(textureDimensions(source)), vec2<f32>(1.0,0.0)), 1.0);
}
@fragment fn gaussian_vertical(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = p.xy / vec2<f32>(textureDimensions(source));
    var value = blur(uv, vec2<f32>(0.0,1.0)) * vec3<f32>(TINT_R,TINT_G,TINT_B) * WEIGHT;
    if FOLD { value += textureSampleLevel(lower, linear_sampler, uv, 0.0).rgb; }
    return vec4<f32>(value, 1.0);
}
