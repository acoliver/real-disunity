// Temporal antialiasing resolve and history copy. The algorithm is documented, with its
// conventions, in taa_reference.rs, which the GPU tests compare this shader against. It follows
// Karis 2014, "High Quality Temporal Supersampling" (luminance-compressed color, 1 / (1 + luma)
// blend weights) and Salvi 2016, "An Excursion in Temporal Supersampling" (mean plus or minus a
// multiple of the standard deviation as the color box). Written from those descriptions.
//
// Pixel (x, y) has its centre at (x + 0.5, y + 0.5), y down. The current color holds samples
// rendered with the projection shifted by `jitter` pixels, so pixel q holds the scene at
// q + 0.5 - jitter; history is on the unjittered grid. Depth is
// single-sample device depth 0..1, 1 is far. NDC y is up: uv.y = 0.5 - 0.5 * ndc.y.
struct Taa {
    // Previous unjittered view-projection times the inverse current one, column-major.
    reprojection: mat4x4<f32>,
    // xy: jitter in pixels (+y down), z: 1 when history is valid, w: padding.
    jitter_valid: vec4<f32>,
}
@group(0) @binding(0) var color: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var history: texture_2d<f32>;
@group(0) @binding(3) var history_sampler: sampler;
@group(0) @binding(4) var<uniform> taa: Taa;
override HISTORY_WEIGHT: f32 = 0.9;
override CLIP_SCALE: f32 = 1.0;
// Samples per depth texel. The multisampled build of this module, which declares `depth` as
// texture_depth_multisampled_2d, sets it to the frame's sample count.
override SAMPLES: i32 = 1;
// Gaussian sigma, in pixels, of the unjitter reconstruction.
const KERNEL_SIGMA: f32 = 0.47;
const MIN_BOX_EXTENT: f32 = 1e-4;
const MAX_COMPRESSED_LUMA: f32 = 0.99999;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
// WGSL does not promise NaN or infinity comparisons, so test the exponent bits.
fn finite(v: f32) -> bool {
    return (bitcast<u32>(v) & 0x7f800000u) != 0x7f800000u;
}
fn clean(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        select(0.0, max(v.x, 0.0), finite(v.x)),
        select(0.0, max(v.y, 0.0), finite(v.y)),
        select(0.0, max(v.z, 0.0), finite(v.z)),
        select(0.0, max(v.w, 0.0), finite(v.w)),
    );
}
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA);
}
fn compress(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 + luma(c));
}
fn decompress(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 - clamp(luma(c), 0.0, MAX_COMPRESSED_LUMA));
}
fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}
fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}
// Moves `h` toward the box centre `m` along the line between them until it is inside the box
// of half-extent `e`, instead of clamping each channel.
fn clip_toward_centre(h: vec3<f32>, m: vec3<f32>, e: vec3<f32>) -> vec3<f32> {
    let offset = h - m;
    let ratio = abs(offset) / max(e, vec3<f32>(MIN_BOX_EXTENT));
    let t = max(max(ratio.x, ratio.y), max(ratio.z, 1.0));
    return m + offset / t;
}
fn load_color(p: vec2<i32>, size: vec2<i32>) -> vec4<f32> {
    return clean(textureLoad(color, clamp(p, vec2<i32>(0), size - 1), 0));
}
// The depth of a texel: the smallest over its samples, the one that occludes the others.
fn depth_at(q: vec2<i32>) -> f32 {
    var d = textureLoad(depth, q, 0);
    for (var s = 1; s < SAMPLES; s++) {
        d = min(d, textureLoad(depth, q, s));
    }
    return d;
}
// The texel of the 3x3 neighborhood with the smallest depth; the first wins a tie.
fn closest(p: vec2<i32>, size: vec2<i32>) -> vec2<i32> {
    var best = clamp(p, vec2<i32>(0), size - 1);
    var best_depth = depth_at(best);
    for (var i = 0; i < 9; i++) {
        let q = clamp(p + vec2<i32>(i % 3 - 1, i / 3 - 1), vec2<i32>(0), size - 1);
        let d = depth_at(q);
        if d < best_depth {
            best = q;
            best_depth = d;
        }
    }
    return best;
}
// The history uv for the pixel (xy) and whether it is usable (z): the motion of the
// closest-depth neighbor, applied at the pixel centre.
fn history_uv(p: vec2<i32>, size: vec2<i32>) -> vec3<f32> {
    let size_f = vec2<f32>(size);
    let q = closest(p, size);
    let texel = (vec2<f32>(q) + 0.5) / size_f;
    let jitter = taa.jitter_valid.xy;
    // The depth texel was rasterised `jitter` pixels off its centre.
    let ndc = vec2<f32>(
        2.0 * texel.x - 1.0 - 2.0 * jitter.x / size_f.x,
        1.0 - 2.0 * texel.y + 2.0 * jitter.y / size_f.y,
    );
    let clip = taa.reprojection * vec4<f32>(ndc, depth_at(q), 1.0);
    let previous = vec2<f32>(0.5 + 0.5 * clip.x / clip.w, 0.5 - 0.5 * clip.y / clip.w);
    let unjittered = vec2<f32>(0.5 + 0.5 * ndc.x, 0.5 - 0.5 * ndc.y);
    let uv = (vec2<f32>(p) + 0.5) / size_f + previous - unjittered;
    let usable = finite(clip.w) && clip.w > 0.0 && finite(uv.x) && finite(uv.y)
        && all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    return vec3<f32>(uv, select(0.0, 1.0, usable));
}
@fragment fn taa_resolve(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(position.xy);
    let size = vec2<i32>(textureDimensions(color));
    let jitter = taa.jitter_valid.xy;
    var samples: array<vec3<f32>, 9>;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    var mean = vec3<f32>(0.0);
    for (var i = 0; i < 9; i++) {
        let d = vec2<i32>(i % 3 - 1, i / 3 - 1);
        let s = load_color(p + d, size);
        let offset = vec2<f32>(d) - jitter;
        let w = exp(-dot(offset, offset) / (2.0 * KERNEL_SIGMA * KERNEL_SIGMA));
        sum += w * s;
        total += w;
        samples[i] = to_ycocg(compress(s.rgb));
        mean += samples[i] / 9.0;
    }
    let current = sum / total;
    var variance = vec3<f32>(0.0);
    for (var i = 0; i < 9; i++) {
        let d = samples[i] - mean;
        variance += d * d / 9.0;
    }
    let target_uv = history_uv(p, size);
    let fetched = textureSampleLevel(history, history_sampler, target_uv.xy, 0.0).rgb;
    let fetched_ok = finite(fetched.x) && finite(fetched.y) && finite(fetched.z);
    var rgb = current.rgb;
    if taa.jitter_valid.z > 0.5 && target_uv.z > 0.5 && fetched_ok {
        let h = to_ycocg(compress(max(fetched, vec3<f32>(0.0))));
        let clipped = clip_toward_centre(h, mean, CLIP_SCALE * sqrt(variance));
        let history_rgb = decompress(from_ycocg(clipped));
        let wc = (1.0 - HISTORY_WEIGHT) / (1.0 + max(luma(current.rgb), 0.0));
        let wh = HISTORY_WEIGHT / (1.0 + max(luma(history_rgb), 0.0));
        rgb = (current.rgb * wc + history_rgb * wh) / (wc + wh);
    }
    return clean(vec4<f32>(rgb, current.a));
}
// Copies the resolve output, bound as `color`, into the history texture.
@fragment fn taa_history(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(color, vec2<i32>(position.xy), 0);
}
