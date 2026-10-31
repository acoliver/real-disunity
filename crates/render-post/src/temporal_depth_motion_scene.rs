//! The analytic scene of the toward and away measurement. One description feeds the f64 CPU
//! function that builds the reference and the WGSL source that rasterises the internal frame.
//!
//! On the fixed 160 x 96 canvas a square bar of constant colour 0.5 (the mean of the
//! checkerboard behind it, so the neighbourhood colour clip cannot tell it from the background)
//! is centred at `CENTRE`. It stands for a surface that moves along the view axis: its half
//! size `half(frame)` grows by one pixel per frame while it approaches and shrinks by one while
//! it recedes, and its device depth falls from 0.5 to 0.3 as it grows.
//! The checkerboard (12 pixel cells between 0.3 and 0.7) sits at depth 0.8.
//!
//! The rasteriser writes the object residual of the bar: a bar point at offset `d` from the
//! centre was at `d * (half - rate) / half` a frame ago, so its backward motion in uv units is
//! `-d * rate / half` over the canvas size. The background is static.

pub(crate) const CANVAS: [f64; 2] = [160.0, 96.0];
pub(crate) const CENTRE: [f64; 2] = [80.0, 48.0];
/// Half size of the bar at frame 0, in canvas pixels.
const START: f64 = 12.0;
/// Frames of approach; the bar recedes for the same number afterwards.
pub(crate) const APPROACH_FRAMES: u64 = 24;
const BAR_VALUE: f64 = 0.5;
const CELL: f64 = 12.0;
const CHECKER: [f64; 2] = [0.3, 0.7];
const BACKGROUND_DEPTH: f64 = 0.8;
/// Device depth of the bar at its smallest and at its largest.
const FAR_DEPTH: f64 = 0.5;
const NEAR_DEPTH: f64 = 0.3;

#[expect(
    clippy::cast_precision_loss,
    reason = "frame indices of a test sequence are far below 2^52"
)]
fn frames(frame: u64) -> f64 {
    frame as f64
}

/// Half size of the bar at `frame`.
pub(crate) fn half(frame: u64) -> f64 {
    let up = frame.min(APPROACH_FRAMES);
    let down = frame.saturating_sub(APPROACH_FRAMES);
    START + frames(up) - frames(down)
}

/// Growth of the half size in the last frame: +1 approaching, -1 receding, 0 on frame 0.
pub(crate) fn rate(frame: u64) -> f64 {
    if frame == 0 {
        0.0
    } else {
        half(frame) - half(frame - 1)
    }
}

/// Device depth of the bar when its half size is `half`.
pub(crate) fn depth(half: f64) -> f64 {
    let grown = (half - START) / frames(APPROACH_FRAMES);
    FAR_DEPTH + (NEAR_DEPTH - FAR_DEPTH) * grown
}

/// Chebyshev distance of `p` from the centre of the bar.
pub(crate) fn distance(p: [f64; 2]) -> f64 {
    (p[0] - CENTRE[0]).abs().max((p[1] - CENTRE[1]).abs())
}

fn checker(p: [f64; 2]) -> f64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices of a 160 x 96 canvas are tiny"
    )]
    let cell = |v: f64| (v / CELL).floor() as i64;
    CHECKER[usize::from((cell(p[0]) + cell(p[1])).rem_euclid(2) == 1)]
}

/// Scene-referred value at canvas position `p` when the bar has half size `half`.
pub(crate) fn value(p: [f64; 2], half: f64) -> f64 {
    if distance(p) < half {
        BAR_VALUE
    } else {
        checker(p)
    }
}

/// WGSL that rasterises the scene at the internal extent and the jittered sample position of each
/// pixel. The uniform holds the internal extent, the jitter in internal pixels, and in `bar` the
/// half size, the growth per frame, the bar depth and the background depth.
pub(crate) fn wgsl() -> String {
    format!(
        "
struct Frame {{ render: vec2<f32>, jitter: vec2<f32>, bar: vec4<f32> }}
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
    let canvas = vec2<f32>({w:?}, {h:?});
    let p = (frag.xy - frame.jitter) / frame.render * canvas;
    let d = p - vec2<f32>({cx:?}, {cy:?});
    var o: Out;
    if max(abs(d.x), abs(d.y)) < frame.bar.x {{
        o.color = vec4<f32>({BAR_VALUE:?}, {BAR_VALUE:?}, {BAR_VALUE:?}, 1.0);
        o.motion = -d * frame.bar.y / frame.bar.x / canvas;
        o.depth = frame.bar.z;
        return o;
    }}
    var v = {c0:?};
    if ((i32(floor(p.x / {CELL:?})) + i32(floor(p.y / {CELL:?}))) & 1) == 1 {{ v = {c1:?}; }}
    o.color = vec4<f32>(v, v, v, 1.0);
    o.motion = vec2<f32>(0.0);
    o.depth = {BACKGROUND_DEPTH:?};
    return o;
}}
",
        w = CANVAS[0],
        h = CANVAS[1],
        cx = CENTRE[0],
        cy = CENTRE[1],
        c0 = CHECKER[0],
        c1 = CHECKER[1],
    )
}
