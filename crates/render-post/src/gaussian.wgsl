@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var lower: texture_2d<f32>;
struct Weights { taps: array<vec2<f32>, 33> }
@group(0) @binding(3) var<storage, read> weights: Weights;
@group(0) @binding(4) var<storage, read> exposure: f32;
override QUARTER: bool = false;
override MAXIMUM: f32 = 65504.0;
override LUM_R: f32 = 0.2126;
override LUM_G: f32 = 0.7152;
override LUM_B: f32 = 0.0722;
override EXPOSURE_SCALE: f32 = 1.0;
override HOST_EXPOSURE: f32 = 1.0;
override CONTRAST: f32 = 1.0;
override EPSILON: f32 = 0.00001;
override REINHARD: bool = false;
override HORIZONTAL_TINT: bool = false;
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
fn soft_bright(c: vec3<f32>) -> vec3<f32> {
    let v = vec3<f32>(clean(c.r), clean(c.g), clean(c.b));
    let brightness = max(v.r, max(v.g, v.b));
    let knee = THRESHOLD * KNEE;
    var soft = clamp(brightness - THRESHOLD + knee, 0.0, 2.0 * knee);
    soft = soft * soft / max(4.0 * knee, 0.00001);
    return v * (max(brightness - THRESHOLD, soft) / max(brightness, 0.00001));
}
fn bright(p: vec2<i32>) -> vec3<f32> { return soft_bright(load(p)); }
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
    // Direct storage reads keep dynamic indexing out of by-value tap arrays.
    var value = textureSampleLevel(source, linear_sampler, uv, 0.0).rgb * weights.taps[0].y;
    for (var i = 1u; i <= TAPS; i++) { value += sample_pair(uv, step, weights.taps[i]); }
    return value;
}
@fragment fn gaussian_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    var value = blur(p.xy / vec2<f32>(textureDimensions(source)), vec2<f32>(1.0,0.0));
    if HORIZONTAL_TINT { value *= vec3<f32>(TINT_R,TINT_G,TINT_B); }
    return vec4<f32>(value, 1.0);
}
@fragment fn gaussian_vertical(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = p.xy / vec2<f32>(textureDimensions(source));
    var value = blur(uv, vec2<f32>(0.0,1.0));
    if !HORIZONTAL_TINT { value *= vec3<f32>(TINT_R,TINT_G,TINT_B); }
    value *= WEIGHT;
    if FOLD { value += textureSampleLevel(lower, linear_sampler, uv, 0.0).rgb; }
    return vec4<f32>(value, 1.0);
}

fn quarter_average(p: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(source));
    let reduced_size = ceil(size * 0.5);
    let uv = p / reduced_size;
    let d = vec2<f32>(0.25) / size;
    return (textureSampleLevel(source, linear_sampler, uv + d, 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv - d, 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv + vec2<f32>(d.x,-d.y), 0.0).rgb
        + textureSampleLevel(source, linear_sampler, uv + vec2<f32>(-d.x,d.y), 0.0).rgb) * 0.25;
}
fn curve(x: f32) -> f32 {
    if REINHARD { return x / (1.0 + x); }
    return x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14);
}
fn tone_bright(c: vec3<f32>) -> vec3<f32> {
    let v = min(vec3<f32>(clean(c.r),clean(c.g),clean(c.b)), vec3<f32>(MAXIMUM));
    let lum = dot(v, vec3<f32>(LUM_R,LUM_G,LUM_B));
    let s = max(curve(clean(exposure) * EXPOSURE_SCALE * HOST_EXPOSURE * lum) - curve(THRESHOLD), 0.0) * CONTRAST;
    return v * s / max(max(v.r,max(v.g,v.b)), EPSILON);
}
@fragment fn gaussian_filtered_bright(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(soft_bright(quarter_average(p.xy)), 1.0);
}
@fragment fn gaussian_filtered_downsample(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(quarter_average(p.xy), 1.0);
}
@fragment fn gaussian_tone_bright(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    if QUARTER { return vec4<f32>(tone_bright(quarter_average(p.xy)), 1.0); }
    let q = vec2<i32>(p.xy) * 2;
    return vec4<f32>((tone_bright(load(q)) + tone_bright(load(q+vec2<i32>(1,0)))
        + tone_bright(load(q+vec2<i32>(0,1))) + tone_bright(load(q+vec2<i32>(1)))) * 0.25, 1.0);
}
