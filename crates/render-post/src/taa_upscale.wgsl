// Temporal reconstruction on the output grid. The algorithm and its conventions are in
// temporal_upscale_reference.rs, which the GPU tests compare this shader against.
//
// The current color is an internal image of extent R rendered with the projection shifted by
// `jitter` internal pixels; the sample stored at integer texel s lies at (s + 0.5 - jitter) / R.
// Output pixel p, u = (p + 0.5) / O, reads the current color at q = u * R + jitter - 0.5 in
// texel-index coordinates through a normalised Lanczos2 over the 4x4 texels around q. History
// and output are on the O grid. History is read with sixteen exact texel loads, not bilinear
// taps, so its weights do not depend on the sampler's fixed-point precision.
struct Temporal {
    // xy: R, zw: O.
    extents: vec4<f32>,
    // xy: jitter in internal pixels (+y down), z: jitter phase count, w: history weight.
    jitter_phase_weight: vec4<f32>,
    // x: clip scale, y: 1 when history holds the previous frame, zw: padding.
    clip_valid: vec4<f32>,
}
// Current color, Rgba16Float at R.
@group(0) @binding(0) var color: texture_2d<f32>;
// Accumulated history, Rgba16Float at O.
@group(0) @binding(1) var history: texture_2d<f32>;
// Total backward motion in uv units, Rg32Float at R: the camera term and the object residual of
// the nearest surface of the 3x3 neighbourhood, from taa_reconstruct.wgsl. Infinite components
// mark history that cannot be used. The history position of output pixel p is u plus the motion
// read at the internal texel nearest q.
@group(0) @binding(2) var motion: texture_2d<f32>;
@group(0) @binding(3) var<uniform> temporal: Temporal;
// Confidence of the history, 1.0 or 0.0, R32Float at R, from the depth clip of
// taa_reconstruct.wgsl.
@group(0) @binding(4) var confidence: texture_2d<f32>;
// Thin-feature candidates, Rg32Float at R from taa_thin.wgsl: x strength, y compressed luminance.
@group(0) @binding(5) var candidates: texture_2d<f32>;
// Lock bank of the previous frame, Rgba16Float at O: x frames left, y reference luminance of the
// neighbourhood, z contrast of the feature, w unused. All zero means no lock.
@group(0) @binding(6) var lock_history: texture_2d<f32>;
// 1 enables the thin-feature locks, 0 is the path before them (a test switch, 1 in production).
override locks: f32 = 1.0;
// Same constant as the native resolve: weight scale 1 - MOTION_K * displacement in output pixels.
const MOTION_K: f32 = 1.5;
const HALF_PI: f32 = 1.5707963267948966;
// A locked texel halves the share of the new sample: the history weight w becomes 1 - (1 - w) / 2,
// which doubles the frames the accumulation averages over. A history weight of 0 stays 0.
const LOCK_SHARE: f32 = 0.5;
struct Resolved {
    @location(0) color: vec4<f32>,
    @location(1) lock: vec4<f32>,
}
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
// Lanczos kernel of radius two, sinc(x) * sinc(x / 2), as a polynomial without builtin
// transcendentals. With a = pi * x / 2 in [0, pi]: sinc(x) * sinc(x / 2) = (sin a / a)^2 * cos a.
// Both factors are Taylor polynomials in t = a^2 up to degree 10, truncated beyond
// pi^22 / 22! < 8e-11. Zero at and beyond two texels.
fn lanczos2(x: f32) -> f32 {
    let ax = min(abs(x), 2.0);
    let t = HALF_PI * HALF_PI * ax * ax;
    let s = 1.0 + t * (-1.6666667e-1 + t * (8.3333333e-3 + t * (-1.9841270e-4
        + t * (2.7557319e-6 + t * (-2.5052108e-8 + t * (1.6059044e-10
        + t * (-7.6471637e-13 + t * (2.8114573e-15 + t * (-8.2206352e-18
        + t * 1.9572941e-20)))))))));
    let c = 1.0 + t * (-0.5 + t * (4.1666667e-2 + t * (-1.3888889e-3
        + t * (2.4801587e-5 + t * (-2.7557319e-7 + t * (2.0876757e-9
        + t * (-1.1470746e-11 + t * (4.7794773e-14 + t * (-1.5619207e-16
        + t * 4.1103176e-19)))))))));
    return select(s * s * c, 0.0, abs(x) >= 2.0);
}
fn load_color(p: vec2<i32>) -> vec4<f32> {
    let last = vec2<i32>(textureDimensions(color)) - 1;
    return clean(textureLoad(color, clamp(p, vec2<i32>(0), last), 0));
}
fn load_history(p: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(history)) - 1;
    return textureLoad(history, clamp(p, vec2<i32>(0), last), 0).rgb;
}
// Normalised Lanczos2 of the 4x4 texels around q.
fn reconstruct(q: vec2<f32>) -> vec4<f32> {
    let base = floor(q);
    let origin = vec2<i32>(base);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = 0; i < 16; i++) {
        let d = vec2<i32>(i % 4 - 1, i / 4 - 1);
        let w = lanczos2(q.x - (base.x + f32(d.x))) * lanczos2(q.y - (base.y + f32(d.y)));
        sum += w * load_color(origin + d);
        total += w;
    }
    return sum / total;
}
// Centre and half-extent of the YCoCg box of the 3x3 texels around the one nearest q.
fn neighbourhood(q: vec2<f32>) -> array<vec3<f32>, 2> {
    let centre = vec2<i32>(floor(q + 0.5));
    var lo = vec3<f32>(3.4e38);
    var hi = vec3<f32>(-3.4e38);
    for (var i = 0; i < 9; i++) {
        let s = to_ycocg(compress(load_color(centre + vec2<i32>(i % 3 - 1, i / 3 - 1)).rgb));
        lo = min(lo, s);
        hi = max(hi, s);
    }
    return array<vec3<f32>, 2>(0.5 * (lo + hi), 0.5 * (hi - lo));
}
fn catmull_rom_weights(f: f32) -> vec4<f32> {
    return vec4<f32>(
        f * (-0.5 + f * (1.0 - 0.5 * f)),
        1.0 + f * f * (-2.5 + 1.5 * f),
        f * (0.5 + f * (2.0 - 1.5 * f)),
        f * f * (-0.5 + 0.5 * f),
    );
}
// Catmull-Rom history color at uv over the 4x4 texels around it. Negative lobes can undershoot;
// the caller clamps at zero. Not finite when any contributing texel is not.
fn catmull_rom(uv: vec2<f32>) -> vec3<f32> {
    let position = uv * temporal.extents.zw - 0.5;
    let base = floor(position);
    let wx = catmull_rom_weights(position.x - base.x);
    let wy = catmull_rom_weights(position.y - base.y);
    let origin = vec2<i32>(base);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 16; i++) {
        let d = vec2<i32>(i % 4, i / 4);
        sum += wx[d.x] * wy[d.y] * load_history(origin + d - 1);
    }
    return sum;
}
// Least confidence of the 2x2 internal texels around q: history is used only where all four
// agree that it belongs to the surface.
fn history_confidence(q: vec2<f32>) -> f32 {
    let base = vec2<i32>(floor(q));
    let last = vec2<i32>(textureDimensions(confidence)) - 1;
    var least = 1.0;
    for (var i = 0; i < 4; i++) {
        let t = clamp(base + vec2<i32>(i % 2, i / 2), vec2<i32>(0), last);
        least = min(least, textureLoad(confidence, t, 0).x);
    }
    return least;
}
// Strongest thin-feature candidate of the 2x2 internal texels around q, 0 where the history does
// not belong to the surface (`confidence` 0, which includes failed depth) or the motion is not
// finite.
fn gather_candidate(q: vec2<f32>, flow: vec2<f32>, confidence: f32) -> f32 {
    if !finite(flow.x) || !finite(flow.y) || confidence <= 0.0 {
        return 0.0;
    }
    let base = vec2<i32>(floor(q));
    let last = vec2<i32>(textureDimensions(candidates)) - 1;
    var best = 0.0;
    for (var i = 0; i < 4; i++) {
        let t = clamp(base + vec2<i32>(i % 2, i / 2), vec2<i32>(0), last);
        best = max(best, textureLoad(candidates, t, 0).x);
    }
    return best;
}
// The lock of the previous frame at the history position uv, or none. It dies when the history is
// unusable (which includes a reset), when its confidence is 0, or when the luminance of the
// neighbourhood jumps by more than the contrast of the feature in one frame: jitter moves that
// luminance by at most half the contrast, so a larger jump is a shading change or the feature is
// gone.
fn reproject_lock(uv: vec2<f32>, usable: bool, reference: f32) -> vec4<f32> {
    if !usable {
        return vec4<f32>(0.0);
    }
    let last = vec2<i32>(textureDimensions(lock_history)) - 1;
    let at = clamp(vec2<i32>(floor(uv * temporal.extents.zw)), vec2<i32>(0), last);
    let prev = textureLoad(lock_history, at, 0);
    let alive = prev.x > 0.5 && finite(prev.y) && abs(reference - prev.y) <= prev.z;
    return select(vec4<f32>(0.0), prev, alive);
}
// The lock written for this frame: refreshed to the full lifetime (one jitter cycle) by a
// candidate, aged by one frame otherwise, with the reference following slow drift.
fn next_lock(prev: vec4<f32>, candidate: f32, reference: f32) -> vec4<f32> {
    if candidate > 0.0 {
        return vec4<f32>(temporal.jitter_phase_weight.z, reference, candidate, 0.0);
    }
    if prev.x > 1.5 {
        return vec4<f32>(prev.x - 1.0, reference, prev.z, 0.0);
    }
    return vec4<f32>(0.0);
}
// What both entry points compute before the locks: the reconstruction, the history position and
// its validity, the confidence and the neighbourhood box.
struct Prepared {
    q: vec2<f32>,
    flow: vec2<f32>,
    uv: vec2<f32>,
    usable: bool,
    confidence: f32,
    bounds: array<vec3<f32>, 2>,
    current: vec4<f32>,
}
fn prepare(position: vec4<f32>) -> Prepared {
    let in_extent = temporal.extents.xy;
    let out_extent = temporal.extents.zw;
    let u = (floor(position.xy) + 0.5) / out_extent;
    let q = fma(u, in_extent, temporal.jitter_phase_weight.xy) - 0.5;
    let texel = clamp(vec2<i32>(floor(q + 0.5)), vec2<i32>(0), vec2<i32>(in_extent) - 1);
    let flow = textureLoad(motion, texel, 0).xy;
    let uv = u + flow;
    let usable = temporal.clip_valid.y > 0.5 && finite(uv.x) && finite(uv.y)
        && all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    return Prepared(q, flow, uv, usable, history_confidence(q), neighbourhood(q), reconstruct(q));
}
// The resolved color. `locked` halves the share of the new sample.
fn blend(s: Prepared, locked: bool) -> vec4<f32> {
    let out_extent = temporal.extents.zw;
    var rgb = s.current.rgb;
    if s.usable {
        let fetched = catmull_rom(s.uv);
        if finite(fetched.x) && finite(fetched.y) && finite(fetched.z) {
            let h = to_ycocg(compress(max(fetched, vec3<f32>(0.0))));
            let clipped = clip_toward_centre(h, s.bounds[0], temporal.clip_valid.x * s.bounds[1]);
            let moved = length(s.flow * out_extent);
            let base = temporal.jitter_phase_weight.w;
            let weight = select(base, 1.0 - (1.0 - base) * LOCK_SHARE, locked && base > 0.0);
            let wh = weight * clamp(1.0 - MOTION_K * moved, 0.0, 1.0) * s.confidence;
            rgb = s.current.rgb * (1.0 - wh) + decompress(from_ycocg(clipped)) * wh;
        }
    }
    return clean(vec4<f32>(rgb, s.current.a));
}
// The entry point with the thin-feature locks: the only one that reads the candidates and the lock
// history and writes the lock bank.
@fragment fn taa_upscale(@builtin(position) position: vec4<f32>) -> Resolved {
    let s = prepare(position);
    let locking = locks > 0.5;
    let prev = reproject_lock(s.uv, s.usable && locking && s.confidence > 0.0, s.bounds[0].x);
    let candidate = select(0.0, gather_candidate(s.q, s.flow, s.confidence), locking);
    let lock = next_lock(prev, candidate, s.bounds[0].x);
    return Resolved(blend(s, lock.x > 0.0), lock);
}
// The entry point without locks: binds neither the candidates nor the lock history and has one
// target.
@fragment fn taa_upscale_unlocked(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return blend(prepare(position), false);
}
