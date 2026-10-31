//! Analytic scene of the camera dolly test: a checkered plane that leans away to the right, seen
//! by a pinhole camera that moves along its view axis. One description feeds the f64 CPU function
//! that builds the reference and the WGSL source that rasterises the internal frame, so the two
//! cannot drift apart.
//!
//! The camera looks along +Z and has `ndc = (X, Y) / d` with `d` the distance along the view axis.
//! The plane is `Z = DISTANCE + SLOPE * X`. For a camera advanced by `c` along Z the ray of NDC
//! position `n` hits it at `d = (DISTANCE - c) / (1 - SLOPE * n.x)`, with plane coordinates
//! `(n.x * d, n.y * d)` carrying a checker of `CELL` units. Depth is standard Z with
//! `NEAR` and `FAR`: device depth `A + B / d`. The distance along the ray varies by a factor of
//! `(1 + SLOPE) / (1 - SLOPE)` across the frame, so the reprojection of a dolly step differs per
//! pixel and cannot be reproduced by one constant depth.

/// Canvas size in output pixels.
pub(crate) const CANVAS: [f64; 2] = [160.0, 96.0];
pub(crate) const DISTANCE: f64 = 2.0;
pub(crate) const SLOPE: f64 = 0.4;
pub(crate) const CELL: f64 = 0.25;
pub(crate) const VALUES: [f64; 2] = [0.2, 0.8];
pub(crate) const NEAR: f64 = 0.1;
pub(crate) const FAR: f64 = 10.0;

/// Device depth is `DEPTH_A + DEPTH_B / d`.
pub(crate) const DEPTH_A: f64 = FAR / (FAR - NEAR);
pub(crate) const DEPTH_B: f64 = -FAR * NEAR / (FAR - NEAR);

/// Scene-referred value at canvas position `p` for a camera advanced by `advance`.
pub(crate) fn value(p: [f64; 2], advance: f64) -> f64 {
    let n = [2.0 * p[0] / CANVAS[0] - 1.0, 1.0 - 2.0 * p[1] / CANVAS[1]];
    let d = (DISTANCE - advance) / (1.0 - SLOPE * n[0]);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices of this scene are tiny"
    )]
    let cell = |v: f64| (v / CELL).floor() as i64;
    let parity = (cell(n[0] * d) + cell(n[1] * d)).rem_euclid(2);
    VALUES[usize::from(parity == 1)]
}

/// WGSL that rasterises the scene at the internal extent and the jittered sample position of each
/// pixel, writing colour, zero object motion and device depth. The uniform holds the internal
/// extent, the jitter in internal pixels and the camera advance in `camera.x`.
pub(crate) fn wgsl() -> String {
    format!(
        "
struct Frame {{ render: vec2<f32>, jitter: vec2<f32>, camera: vec4<f32> }}
@group(0) @binding(0) var<uniform> frame: Frame;
struct Out {{
    @location(0) color: vec4<f32>,
    @location(1) motion: vec2<f32>,
    @builtin(frag_depth) depth: f32,
}}
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}}
@fragment fn paint(@builtin(position) frag: vec4<f32>) -> Out {{
    let p = (frag.xy - frame.jitter) / frame.render * vec2<f32>({w:?}, {h:?});
    let n = vec2<f32>(2.0 * p.x / {w:?} - 1.0, 1.0 - 2.0 * p.y / {h:?});
    let d = ({DISTANCE:?} - frame.camera.x) / (1.0 - {SLOPE:?} * n.x);
    let cell = vec2<i32>(floor(n * d / {CELL:?}));
    var v = {c0:?};
    if ((cell.x + cell.y) & 1) != 0 {{ v = {c1:?}; }}
    var o: Out;
    o.color = vec4<f32>(v, v, v, 1.0);
    o.motion = vec2<f32>(0.0);
    o.depth = {DEPTH_A:?} + {DEPTH_B:?} / d;
    return o;
}}
",
        w = CANVAS[0],
        h = CANVAS[1],
        c0 = VALUES[0],
        c1 = VALUES[1],
    )
}
