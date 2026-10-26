@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var lower: texture_2d<f32>;
struct Exposure { value: f32, initialized: u32, pad: vec2<u32> }
@group(0) @binding(2) var<storage, read> exposure: Exposure;
override THRESHOLD: f32 = 1.0;
override KNEE: f32 = 0.5;
override GAUSSIAN: bool = false;
override INTENSITY: f32 = 0.0;
override FIXED: f32 = 1.0;
override ADAPTIVE: bool = false;
override FXAA: f32 = 0.75;
override CURVE: u32 = 0u;
override GRADE_COMPOSITION: bool = false;
override EDGE_THRESHOLD: f32 = 0.1;
override MINIMUM_THRESHOLD: f32 = 1.0 / 64.0;
override SUBPIXEL_BLEND: f32 = 1.0;
override SRGB_LUMA: bool = false;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn clean(v: f32) -> f32 {
    let b = bitcast<u32>(v);
    if (b & 0x80000000u) != 0u || (b & 0x7f800000u) == 0x7f800000u { return 0.0; }
    return v;
}
fn load(t: texture_2d<f32>, p: vec2<i32>) -> vec4<f32> {
    return textureLoad(t, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(t)) - 1), 0);
}
fn linear(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let p = uv * vec2<f32>(textureDimensions(t)) - 0.5;
    let q = vec2<i32>(floor(p)); let f = fract(p);
    return mix(mix(load(t,q), load(t,q+vec2<i32>(1,0)), f.x), mix(load(t,q+vec2<i32>(0,1)), load(t,q+vec2<i32>(1)), f.x), f.y);
}
@fragment fn prefilter(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let incoming = load(source, vec2<i32>(pixel.xy));
    let rgb = vec3<f32>(clean(incoming.r), clean(incoming.g), clean(incoming.b));
    let brightness = max(rgb.r, max(rgb.g, rgb.b));
    let radius = THRESHOLD * KNEE;
    var extracted = max(brightness - THRESHOLD, 0.0);
    if radius > 0.0 {
        let ramp = clamp(brightness - THRESHOLD + radius, 0.0, 2.0 * radius);
        extracted = max(extracted, ramp * ramp / (4.0 * radius));
    }
    var rgb_scale = 0.0;
    if brightness > 0.0 { rgb_scale = extracted / brightness; }
    return vec4<f32>(rgb * rgb_scale, 1.0);
}

// The outer product of [1, 2, 1] sums to 16 and preserves constant radiance.
fn tent_reconstruction(image: texture_2d<f32>, center: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(image));
    var accumulated = vec3<f32>(0.0);
    for (var row = -1; row <= 1; row += 1) {
        for (var column = -1; column <= 1; column += 1) {
            let horizontal_weight = select(1.0, 2.0, column == 0);
            let vertical_weight = select(1.0, 2.0, row == 0);
            let location = center + vec2<f32>(f32(column), f32(row)) * texel;
            accumulated += linear(image, location).rgb * (horizontal_weight * vertical_weight / 16.0);
        }
    }
    return accumulated;
}

@fragment fn downsample(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let extent = (textureDimensions(source) + vec2<u32>(1u)) / 2u;
    let center = pixel.xy / vec2<f32>(extent);
    return vec4<f32>(tent_reconstruction(source, center), 1.0);
}

@fragment fn upsample(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let center = pixel.xy / vec2<f32>(textureDimensions(source));
    let retained = linear(source, center).rgb;
    let reconstructed = tent_reconstruction(lower, center);
    return vec4<f32>((retained + reconstructed) * 0.5, 1.0);
}
fn mapped(r: f32, e: f32) -> f32 {
    let b = bitcast<u32>(r);
    if e == 0.0 || (b & 0x80000000u) != 0u || (b & 0x7fffffffu) > 0x7f800000u { return 0.0; }
    if b == 0x7f800000u { return 1.0; }
    let v = r * e;
    if CURVE == 1u { return aces_display(v); }
    return v / (1.0 + v);
}
@fragment fn tone(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = load(source,vec2<i32>(p.xy));
    var bloom = vec3<f32>(0.0);
    if INTENSITY > 0.0 {
        bloom = load(lower,vec2<i32>(p.xy)).rgb;
        if GAUSSIAN { bloom = linear(lower, p.xy / vec2<f32>(textureDimensions(source))).rgb; }
        bloom *= INTENSITY;
    }
    let e = select(FIXED, exposure.value, ADAPTIVE);
    if CURVE == 2u || GRADE_COMPOSITION {
        var v = vec3<f32>(0.0);
        if LINEAR_SCALE != 0.0 {
            var exposed = vec3<f32>(0.0);
            if e != 0.0 { exposed = vec3<f32>(positive_hdr(c.r),positive_hdr(c.g),positive_hdr(c.b)) * e; }
            v = (exposed + bloom) * LINEAR_SCALE;
        }
        var result = hill_display(v);
        if CURVE != 2u { result = vec3<f32>(mapped(v.r,1.0),mapped(v.g,1.0),mapped(v.b,1.0)); }
        return vec4<f32>(display_grade(result, p.xy / vec2<f32>(textureDimensions(source))), c.a);
    }
    // Classify original HDR before adding finite bloom, preserving special-value policy.
    let base = vec3<f32>(mapped(c.r,e),mapped(c.g,e),mapped(c.b,e));
    let v = vec3<f32>(clean(c.r),clean(c.g),clean(c.b)) + bloom;
    var result = vec3<f32>(mapped(v.r,e),mapped(v.g,e),mapped(v.b,e));
    if (bitcast<u32>(c.r) & 0x7f800000u) == 0x7f800000u { result.r = base.r; }
    if (bitcast<u32>(c.g) & 0x7f800000u) == 0x7f800000u { result.g = base.g; }
    if (bitcast<u32>(c.b) & 0x7f800000u) == 0x7f800000u { result.b = base.b; }
    return vec4<f32>(display_grade(result, p.xy / vec2<f32>(textureDimensions(source))),c.a);
}
fn edge_brightness(rgb: vec3<f32>) -> f32 {
    let luma = (rgb.r + 2.0 * rgb.g + rgb.b) * 0.25;
    if !SRGB_LUMA { return luma; }
    if luma <= 0.0031308 { return luma * 12.92; }
    return 1.055 * pow(luma, 1.0 / 2.4) - 0.055;
}

@fragment fn fxaa(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let extent = vec2<f32>(textureDimensions(source));
    let center_uv = pixel.xy / extent;
    let original = load(source, vec2<i32>(pixel.xy));
    var darkest = edge_brightness(original.rgb);
    var brightest = darkest;
    var slope = vec2<f32>(0.0);
    for (var row = -1; row <= 1; row += 1) {
        for (var column = -1; column <= 1; column += 1) {
            let offset = vec2<f32>(f32(column), f32(row));
            let brightness = edge_brightness(load(source, vec2<i32>(pixel.xy) + vec2<i32>(column, row)).rgb);
            darkest = min(darkest, brightness);
            brightest = max(brightest, brightness);
            slope.x += brightness * offset.x * select(1.0, 2.0, row == 0);
            slope.y += brightness * offset.y * select(1.0, 2.0, column == 0);
        }
    }
    let major_slope = max(abs(slope.x), abs(slope.y));
    // Preserve the original division and rounding for the default relative cutoff.
    var relative = brightest * EDGE_THRESHOLD;
    if EDGE_THRESHOLD == 0.1 { relative = brightest / 10.0; }
    if brightest - darkest <= max(MINIMUM_THRESHOLD, relative) || major_slope == 0.0 {
        return original;
    }
    let along_edge = vec2<f32>(-slope.y, slope.x) / major_slope;
    let reach = along_edge * 1.5 / extent;
    let before = linear(source, center_uv - reach).rgb;
    let after = linear(source, center_uv + reach).rgb;
    let smoothed = original.rgb * 0.5 + (before + after) * 0.25;
    return vec4<f32>(mix(original.rgb, smoothed, FXAA * SUBPIXEL_BLEND), original.a);
}
