// Temporal antialiasing resolve. The algorithm is documented, with its
// conventions, in taa_reference.rs, which the GPU tests compare this shader against. It follows
// Karis 2014, "High Quality Temporal Supersampling" (luminance-compressed color, 1 / (1 + luma)
// blend weights) with a min/max color box of the 3x3 neighborhood in YCoCg that history is
// clipped to along the ray to the box centre. Written from those descriptions.
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
    // xy: (a, b) of the linear view depth b / (z - a) of device depth z; b == 0 disables the
    // disocclusion test. zw: padding.
    depth_terms: vec4<f32>,
}
@group(0) @binding(0) var color: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var history: texture_2d<f32>;
@group(0) @binding(3) var history_sampler: sampler;
@group(0) @binding(4) var<uniform> taa: Taa;
// Object-motion residual in uv units of the previous frame, Rg16Float:
// uv(P_prev * world_prev) - uv(P_prev * world_now) of the surface a texel shows. It is what the
// camera reprojection of `history_uv` lacks, so 0 (the clear value, and the 1x1 default) leaves
// camera reprojection alone. The multisampled build declares it texture_multisampled_2d.
@group(0) @binding(5) var velocity: texture_2d<f32>;
// Linear view depth of the closest-depth texel of the previous frame, R32Float, same size as
// `color`. It is read with textureLoad only.
@group(0) @binding(6) var depth_history: texture_2d<f32>;
override HISTORY_WEIGHT: f32 = 0.9;
override CLIP_SCALE: f32 = 1.0;
// Samples per depth texel. The multisampled build of this module, which declares `depth` as
// texture_depth_multisampled_2d, sets it to the frame's sample count.
override SAMPLES: i32 = 1;
// Gaussian sigma, in pixels, of the unjitter reconstruction. It is so narrow that only the
// sample nearest the pixel centre counts, so the frames accumulate to the box-filtered pixel.
// Wider kernels blur every frame and the accumulation converges to a blurred image.
const KERNEL_SIGMA: f32 = 0.05;
// Motion-adaptive history weight: the weight is scaled by clamp(1 - k * offset, 0, 1), where
// offset is the reprojection displacement in pixels. A static pixel keeps the full weight; a
// moving one leans on the current frame, so it is resampled from history fewer times. Jittered
// 1x frames need history to supersample, so their k is small; a multisampled frame is already
// antialiased, so history only stabilises it and its k is large (history is dropped above
// 1 / MSAA_MOTION_K pixels). Measured values are in docs/rendering.md.
const MOTION_K: f32 = 1.5;
const MSAA_MOTION_K: f32 = 16.0;
// A history sample is rejected when the depth it should have, predicted from the reprojection,
// lies outside the stored depths of its 2x2 footprint widened by this relative tolerance.
const DEPTH_TOLERANCE: f32 = 0.02;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
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
// The sample of texel `q` with the smallest depth; the first wins a tie. Its velocity goes with
// the depth `depth_at` returns, because the closest sample is the surface that is visible.
fn winning_sample(q: vec2<i32>) -> i32 {
    var best = 0;
    var best_depth = textureLoad(depth, q, 0);
    for (var s = 1; s < SAMPLES; s++) {
        let d = textureLoad(depth, q, s);
        if d < best_depth {
            best = s;
            best_depth = d;
        }
    }
    return best;
}
// The object-motion residual of the closest-depth texel `q`. The load is clamped so that the
// 1x1 zero texture bound when a host has no velocity reads its texel for every `q`.
fn residual_at(q: vec2<i32>) -> vec2<f32> {
    let last = vec2<i32>(textureDimensions(velocity)) - 1;
    return textureLoad(velocity, min(q, last), winning_sample(q)).xy;
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
struct Lookup {
    uv: vec2<f32>,
    usable: bool,
    // Linear depth the closest-depth texel had in the previous frame.
    predicted: f32,
    // Linear depth of the closest-depth texel now, the value stored for the next frame.
    stored: f32,
    // No object-motion residual: the camera reprojection alone moved the texel.
    camera_only: bool,
}
// Linear view depth of device depth `z`; 0 when the depth terms are unset.
fn linear_depth(z: f32) -> f32 {
    let terms = taa.depth_terms;
    return terms.y / (min(z, 1.0) - terms.x);
}
// The history uv for the pixel and whether it is usable: the motion of the
// closest-depth neighbor, applied at the pixel centre. The camera motion comes from depth, the
// object motion from the velocity of that same texel (Karis dilation), so a thin moving edge
// carries its foreground's vector to the pixels around it. A multisampled frame takes both the
// depth and the residual from the pixel itself, with no dilation: its samples already resolve
// the edge, and a neighbor's vector would carry a moving object's history onto the background
// beside it.
fn history_uv(p: vec2<i32>, size: vec2<i32>) -> Lookup {
    let size_f = vec2<f32>(size);
    let q = select(closest(p, size), clamp(p, vec2<i32>(0), size - 1), SAMPLES > 1);
    let texel = (vec2<f32>(q) + 0.5) / size_f;
    let jitter = taa.jitter_valid.xy;
    // The depth texel was rasterised `jitter` pixels off its centre.
    let ndc = vec2<f32>(
        2.0 * texel.x - 1.0 - 2.0 * jitter.x / size_f.x,
        1.0 - 2.0 * texel.y + 2.0 * jitter.y / size_f.y,
    );
    let z = depth_at(q);
    let clip = taa.reprojection * vec4<f32>(ndc, z, 1.0);
    let previous = vec2<f32>(0.5 + 0.5 * clip.x / clip.w, 0.5 - 0.5 * clip.y / clip.w);
    let unjittered = vec2<f32>(0.5 + 0.5 * ndc.x, 0.5 - 0.5 * ndc.y);
    let residual = residual_at(q);
    let uv = (vec2<f32>(p) + 0.5) / size_f + previous - unjittered + residual;
    let usable = finite(clip.w) && clip.w > 0.0 && finite(uv.x) && finite(uv.y)
        && all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    // The reprojection maps NDC, so clip.w is the previous view depth over the current one.
    // Sky is predicted at the far plane, where only camera motion would move it.
    let stored = linear_depth(z);
    let predicted = stored * select(clip.w, 1.0, z >= 1.0);
    return Lookup(uv, usable, predicted, stored, all(residual == vec2<f32>(0.0)));
}
// Whether the surface now visible is not the one in history. Only static or camera-only pixels
// are tested: the residual is XY, so a moving object that changes view depth would fail
// falsely. The prediction must lie in the interval spanned by the stored depths of the 2x2
// footprint at the history uv (wide on steep surfaces, as at grazing angles), widened by the
// tolerance.
fn disoccluded(lookup: Lookup) -> bool {
    if !lookup.camera_only || taa.depth_terms.y == 0.0 || !finite(lookup.predicted) {
        return false;
    }
    let last = vec2<i32>(textureDimensions(depth_history)) - 1;
    let base = vec2<i32>(floor(lookup.uv * vec2<f32>(last + 1) - 0.5));
    var lo = 3.4e38;
    var hi = -3.4e38;
    for (var i = 0; i < 4; i++) {
        let d = textureLoad(depth_history, clamp(base + vec2<i32>(i % 2, i / 2), vec2<i32>(0), last), 0).r;
        lo = min(lo, d);
        hi = max(hi, d);
    }
    return lookup.predicted < lo * (1.0 - DEPTH_TOLERANCE)
        || lookup.predicted > hi * (1.0 + DEPTH_TOLERANCE);
}
// Catmull-Rom (bicubic) history fetch from nine bilinear taps: the two inner texels of each
// axis merge into one tap placed so that the hardware weights reproduce their cubic weights,
// and the outer texels take one tap each. Corner weights are negative, so the sum can undershoot
// and is clamped at zero by the caller.
fn catmull_rom(uv: vec2<f32>, size: vec2<f32>) -> vec3<f32> {
    let position = uv * size;
    let centre = floor(position - 0.5) + 0.5;
    let f = position - centre;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let tap0 = (centre - 1.0) / size;
    let tap12 = (centre + w2 / w12) / size;
    let tap3 = (centre + 2.0) / size;
    var sum = vec3<f32>(0.0);
    sum += fetch(vec2<f32>(tap0.x, tap0.y)) * (w0.x * w0.y);
    sum += fetch(vec2<f32>(tap12.x, tap0.y)) * (w12.x * w0.y);
    sum += fetch(vec2<f32>(tap3.x, tap0.y)) * (w3.x * w0.y);
    sum += fetch(vec2<f32>(tap0.x, tap12.y)) * (w0.x * w12.y);
    sum += fetch(vec2<f32>(tap12.x, tap12.y)) * (w12.x * w12.y);
    sum += fetch(vec2<f32>(tap3.x, tap12.y)) * (w3.x * w12.y);
    sum += fetch(vec2<f32>(tap0.x, tap3.y)) * (w0.x * w3.y);
    sum += fetch(vec2<f32>(tap12.x, tap3.y)) * (w12.x * w3.y);
    sum += fetch(vec2<f32>(tap3.x, tap3.y)) * (w3.x * w3.y);
    return sum;
}
fn fetch(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(history, history_sampler, uv, 0.0).rgb;
}
struct Resolved {
    @location(0) color: vec4<f32>,
    // Linear depth stored for the next frame's disocclusion test.
    @location(1) depth: f32,
}
@fragment fn taa_resolve(@builtin(position) position: vec4<f32>) -> Resolved {
    let p = vec2<i32>(position.xy);
    let size = vec2<i32>(textureDimensions(color));
    let jitter = taa.jitter_valid.xy;
    var samples: array<vec3<f32>, 9>;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = 0; i < 9; i++) {
        let d = vec2<i32>(i % 3 - 1, i / 3 - 1);
        let s = load_color(p + d, size);
        let offset = vec2<f32>(d) - jitter;
        let w = exp(-dot(offset, offset) / (2.0 * KERNEL_SIGMA * KERNEL_SIGMA));
        sum += w * s;
        total += w;
        samples[i] = to_ycocg(compress(s.rgb));
    }
    let current = sum / total;
    var lo = samples[0];
    var hi = samples[0];
    for (var i = 1; i < 9; i++) {
        lo = min(lo, samples[i]);
        hi = max(hi, samples[i]);
    }
    let centre = 0.5 * (lo + hi);
    let half = 0.5 * (hi - lo);
    let target_uv = history_uv(p, size);
    let fetched = catmull_rom(target_uv.uv, vec2<f32>(size));
    let fetched_ok = finite(fetched.x) && finite(fetched.y) && finite(fetched.z);
    var rgb = current.rgb;
    if taa.jitter_valid.z > 0.5 && target_uv.usable && fetched_ok && !disoccluded(target_uv) {
        let h = to_ycocg(compress(max(fetched, vec3<f32>(0.0))));
        let clipped = clip_toward_centre(h, centre, CLIP_SCALE * half);
        let history_rgb = decompress(from_ycocg(clipped));
        let size_f = vec2<f32>(size);
        let motion = length((target_uv.uv - (vec2<f32>(p) + 0.5) / size_f) * size_f);
        let k = select(MOTION_K, MSAA_MOTION_K, SAMPLES > 1);
        let wh = HISTORY_WEIGHT * clamp(1.0 - k * motion, 0.0, 1.0);
        let wc = 1.0 - wh;
        rgb = current.rgb * wc + history_rgb * wh;
    }
    return Resolved(clean(vec4<f32>(rgb, current.a)), select(0.0, target_uv.stored, finite(target_uv.stored)));
}
