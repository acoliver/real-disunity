// Depth and motion reconstruction of the temporal stage at the internal extent R.
//
// `clear` fills the previous-coordinate depth buffer with +infinity bits. `reconstruct` selects
// for every texel the nearest valid depth of its 3x3 neighbourhood (ties go to the first texel in
// raster order), writes that depth and the total backward motion of the winner (camera term at the
// winner's depth plus its object residual), and scatters the depth to the position the winner's
// motion points at in the previous frame: `atomicMin` over the 2x2 texels around it, so where two
// surfaces land on one texel the nearer one stays. Positive f32 bit patterns order like the values.
//
// A depth is valid when it lies strictly between 0 and 1 (standard Z: 0 is an uninitialised or
// clipped sample, 1 the cleared far plane and sky). NaN and infinity fail both comparisons. A
// texel with no valid neighbour writes depth 1 and the total motion of its own residual, and does
// not scatter. A motion that is not finite writes +infinity in both components, which the resolve
// treats as unusable history, and does not scatter. Nothing writes NaN.
struct Camera {
    // Maps (ndc x, ndc y, device depth, 1) of this frame to the previous clip space.
    reprojection: mat4x4<f32>,
    // x: 1 when the camera term applies, y: depth convention (0 is standard Z), zw: depth terms.
    flags: vec4<f32>,
}
@group(0) @binding(0) var depth: texture_depth_2d;
// Object residual of the backward motion in uv units, Rg16Float at R.
@group(0) @binding(1) var motion: texture_2d<f32>;
@group(0) @binding(2) var<uniform> camera: Camera;
@group(0) @binding(3) var out_depth: texture_storage_2d<r32float, write>;
@group(0) @binding(4) var out_motion: texture_storage_2d<rg32float, write>;
// One u32 per internal texel, row major: the nearest depth that reaches it from the current frame.
@group(0) @binding(5) var<storage, read_write> scatter: array<atomic<u32>>;

const INFINITY_BITS: u32 = 0x7f800000u;

fn finite(v: f32) -> bool {
    return (bitcast<u32>(v) & 0x7f800000u) != 0x7f800000u;
}
fn valid_depth(z: f32) -> bool {
    return z > 0.0 && z < 1.0;
}
fn ndc_to_uv(ndc: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(0.5 * ndc.x + 0.5, 0.5 - 0.5 * ndc.y);
}
// Backward uv displacement of the surface at uv `u` and device depth `z` caused by the camera
// alone. Zero without a camera term; infinite when the point is behind the previous camera.
fn camera_motion(u: vec2<f32>, z: f32) -> vec2<f32> {
    if camera.flags.x < 0.5 {
        return vec2<f32>(0.0);
    }
    let ndc = vec2<f32>(2.0 * u.x - 1.0, 1.0 - 2.0 * u.y);
    let clip = camera.reprojection * vec4<f32>(ndc, z, 1.0);
    if !(clip.w > 0.0) {
        return vec2<f32>(bitcast<f32>(INFINITY_BITS));
    }
    return ndc_to_uv(clip.xy / clip.w) - ndc_to_uv(ndc);
}

@compute @workgroup_size(8, 8)
fn clear(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(depth);
    if id.x < size.x && id.y < size.y {
        atomicStore(&scatter[id.y * size.x + id.x], INFINITY_BITS);
    }
}

// Texel of the nearest valid depth around `s`, or `s` itself when none is valid.
fn nearest(s: vec2<i32>, size: vec2<i32>) -> vec2<i32> {
    var best = s;
    var best_depth = 2.0;
    for (var i = 0; i < 9; i++) {
        let t = s + vec2<i32>(i % 3 - 1, i / 3 - 1);
        if all(t >= vec2<i32>(0)) && all(t < size) {
            let z = textureLoad(depth, t, 0);
            if valid_depth(z) && z < best_depth {
                best = t;
                best_depth = z;
            }
        }
    }
    return best;
}

fn scatter_to(p: vec2<f32>, z: f32, size: vec2<i32>) {
    let base = vec2<i32>(floor(p));
    for (var i = 0; i < 4; i++) {
        let t = base + vec2<i32>(i % 2, i / 2);
        if all(t >= vec2<i32>(0)) && all(t < size) {
            atomicMin(&scatter[u32(t.y * size.x + t.x)], bitcast<u32>(z));
        }
    }
}

@compute @workgroup_size(8, 8)
fn reconstruct(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(depth));
    let s = vec2<i32>(id.xy);
    if any(s >= size) {
        return;
    }
    let winner = nearest(s, size);
    let raw = textureLoad(depth, winner, 0);
    let ok = valid_depth(raw);
    let z = select(1.0, raw, ok);
    let u = (vec2<f32>(s) + 0.5) / vec2<f32>(size);
    let residual = textureLoad(motion, winner, 0).xy;
    let flow = camera_motion(u, z) + residual;
    let usable = finite(flow.x) && finite(flow.y);
    let written = select(vec2<f32>(bitcast<f32>(INFINITY_BITS)), flow, usable);
    textureStore(out_depth, s, vec4<f32>(z, 0.0, 0.0, 0.0));
    textureStore(out_motion, s, vec4<f32>(written, 0.0, 0.0));
    if ok && usable {
        scatter_to(vec2<f32>(s) + flow * vec2<f32>(size), z, size);
    }
}
