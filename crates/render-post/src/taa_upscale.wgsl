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
// Object residual of the backward motion in uv units, Rg16Float at R (the format of the velocity
// texture), with camera and jitter already cancelled. The total backward motion is the camera
// term of `camera_motion` plus this; the history position of output pixel p is u plus the total,
// read at the internal texel nearest q.
@group(0) @binding(2) var motion: texture_2d<f32>;
@group(0) @binding(3) var<uniform> temporal: Temporal;
// Camera motion between the frames, see temporal_camera.rs.
struct Camera {
    // Maps (ndc x, ndc y, device depth, 1) of this frame to the previous clip space.
    reprojection: mat4x4<f32>,
    // x: 1 when the camera term applies, y: depth convention (0 is standard Z), zw: depth terms.
    flags: vec4<f32>,
}
@group(0) @binding(4) var<uniform> camera: Camera;
// Single-sample device depth at R.
@group(0) @binding(5) var depth: texture_depth_2d;
fn not_finite() -> f32 {
    return bitcast<f32>(0x7f800000u);
}
// uv of a point in NDC; +y is up in NDC and down in uv.
fn ndc_to_uv(ndc: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(0.5 * ndc.x + 0.5, 0.5 - 0.5 * ndc.y);
}
// Backward uv displacement of the surface at uv `u` and device depth `z` caused by the camera
// alone: where the previous frame's camera saw the same world point, minus `u`. Zero without a
// camera term, and for an identity reprojection exactly, because both positions go through the
// same conversion. Infinite when the point is behind the previous camera, which makes the history
// unusable.
fn camera_motion(u: vec2<f32>, z: f32) -> vec2<f32> {
    if camera.flags.x < 0.5 {
        return vec2<f32>(0.0);
    }
    let ndc = vec2<f32>(2.0 * u.x - 1.0, 1.0 - 2.0 * u.y);
    let clip = camera.reprojection * vec4<f32>(ndc, z, 1.0);
    if !(clip.w > 0.0) {
        return vec2<f32>(not_finite());
    }
    return ndc_to_uv(clip.xy / clip.w) - ndc_to_uv(ndc);
}
// Same constant as the native resolve: weight scale 1 - MOTION_K * displacement in output pixels.
const MOTION_K: f32 = 1.5;
const HALF_PI: f32 = 1.5707963267948966;
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
@fragment fn taa_upscale(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let in_extent = temporal.extents.xy;
    let out_extent = temporal.extents.zw;
    let u = (floor(position.xy) + 0.5) / out_extent;
    let q = fma(u, in_extent, temporal.jitter_phase_weight.xy) - 0.5;
    let current = reconstruct(q);
    let texel = clamp(vec2<i32>(floor(q + 0.5)), vec2<i32>(0), vec2<i32>(in_extent) - 1);
    let flow = camera_motion(u, textureLoad(depth, texel, 0))
        + textureLoad(motion, texel, 0).xy;
    let uv = u + flow;
    let usable = temporal.clip_valid.y > 0.5 && finite(uv.x) && finite(uv.y)
        && all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    var rgb = current.rgb;
    if usable {
        let fetched = catmull_rom(uv);
        if finite(fetched.x) && finite(fetched.y) && finite(fetched.z) {
            let bounds = neighbourhood(q);
            let h = to_ycocg(compress(max(fetched, vec3<f32>(0.0))));
            let clipped = clip_toward_centre(h, bounds[0], temporal.clip_valid.x * bounds[1]);
            let moved = length(flow * out_extent);
            let wh = temporal.jitter_phase_weight.w * clamp(1.0 - MOTION_K * moved, 0.0, 1.0);
            rgb = current.rgb * (1.0 - wh) + decompress(from_ycocg(clipped)) * wh;
        }
    }
    return clean(vec4<f32>(rgb, current.a));
}
