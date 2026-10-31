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
//
// `depth_clip` writes the confidence of the history of every texel, 1 or 0. The texel's dilated
// depth is compared, in linear view depth (`b / (z - a)` with the camera block's depth terms),
// with the nearest depth that reached the texel its motion points at, rounded to the nearest
// texel. A surface that was in front of this one in the previous frame is the nearest and
// differs by more than the tolerance: the history belongs to it, so the confidence is 0. A
// surface alone at its own position finds its own depth there, bit for bit. The tolerance is the
// largest linear depth difference to the 3x3 neighbours of the texel, which the 2x2 scatter
// footprint can reach across, plus 2^-20 relative to the depth precision of the device depth
// there. A texel with no valid depth, or without a camera block or depth terms, has
// confidence 1: nothing says its history is wrong. A motion that points nowhere or at a texel
// nothing reached has confidence 0.
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
// The outputs of `reconstruct`, read by `depth_clip`, and the confidence it writes.
@group(0) @binding(6) var dilated_depth: texture_2d<f32>;
@group(0) @binding(7) var dilated_motion: texture_2d<f32>;
@group(0) @binding(8) var out_confidence: texture_storage_2d<r32float, write>;

// False only in tests: the stage then keeps every texel's own depth and residual, which is the
// temporal path before the dilation existed, so a test can measure both on one device.
override dilate: bool = true;
// False only in tests: every confidence is 1, the temporal path before the depth clip existed.
override clip: bool = true;

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
    if !dilate {
        return s;
    }
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

// Relative bound on the f32 error of a device depth and its raster interpolation: 2^-20.
const DEPTH_PRECISION: f32 = 9.5367431640625e-7;

fn linear_depth(z: f32) -> f32 {
    return camera.flags.w / (z - camera.flags.z);
}
// Largest linear depth difference between the dilated depth `z` at `s` and a valid neighbour of
// its 3x3 neighbourhood, plus the linear depth error of the device depth precision.
fn depth_tolerance(s: vec2<i32>, size: vec2<i32>, z: f32) -> f32 {
    let linear = linear_depth(z);
    var slope = 0.0;
    for (var i = 0; i < 9; i++) {
        let t = s + vec2<i32>(i % 3 - 1, i / 3 - 1);
        if all(t >= vec2<i32>(0)) && all(t < size) {
            let neighbour = textureLoad(dilated_depth, t, 0).x;
            if valid_depth(neighbour) {
                slope = max(slope, abs(linear_depth(neighbour) - linear));
            }
        }
    }
    return slope + DEPTH_PRECISION * abs(linear / (z - camera.flags.z));
}
fn confidence(s: vec2<i32>, size: vec2<i32>) -> f32 {
    let z = textureLoad(dilated_depth, s, 0).x;
    if !clip || camera.flags.x < 0.5 || camera.flags.w == 0.0 || !valid_depth(z) {
        return 1.0;
    }
    let flow = textureLoad(dilated_motion, s, 0).xy;
    if !(finite(flow.x) && finite(flow.y)) {
        return 0.0;
    }
    let t = vec2<i32>(floor(vec2<f32>(s) + flow * vec2<f32>(size) + 0.5));
    if any(t < vec2<i32>(0)) || any(t >= size) {
        return 0.0;
    }
    let reached = bitcast<f32>(atomicLoad(&scatter[u32(t.y * size.x + t.x)]));
    if !valid_depth(reached) {
        return 0.0;
    }
    let apart = abs(linear_depth(z) - linear_depth(reached));
    return select(0.0, 1.0, apart <= depth_tolerance(s, size, z));
}

@compute @workgroup_size(8, 8)
fn depth_clip(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(dilated_depth));
    let s = vec2<i32>(id.xy);
    if any(s >= size) {
        return;
    }
    textureStore(out_confidence, s, vec4<f32>(confidence(s, size), 0.0, 0.0, 0.0));
}
