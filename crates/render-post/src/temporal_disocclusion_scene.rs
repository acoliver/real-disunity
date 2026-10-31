//! The analytic disocclusion scene of the temporal reconstruction measurement. One description
//! feeds the f64 CPU function that builds the reference and the WGSL source that rasterises the
//! internal frame, so the two cannot drift apart.
//!
//! Positions are in output pixels of a fixed 160 x 96 canvas, whatever the extents under test.
//! Three things share the canvas:
//!
//! * a background checkerboard of 12 pixel cells between 0.3 and 0.7, at depth 0.8;
//! * a near bar of constant colour 0.5, the mean of the checkerboard and inside its colour range,
//!   so the neighbourhood colour clip cannot tell it from the background. It is `BAR_SIZE` pixels
//!   wide, moves right by `SPEED` canvas pixels per frame and sits at depth 0.4;
//! * a static plane of horizontal stripes whose period shrinks with the row, standing for a
//!   grazing surface, with depth rising from 0.6 to 0.8 down the rows.
//!
//! The rasteriser writes the correct backward motion in uv units for the bar (minus its velocity over the width
//! in x) and zero elsewhere, which is what the object residual of a real scene would hold for a
//! static camera.

/// Canvas size in output pixels.
pub(crate) const CANVAS: [f64; 2] = [160.0, 96.0];

/// Bar velocity in canvas pixels per frame, +x.
pub(crate) const SPEED: f64 = 2.0;
/// Bar left edge at frame 0, width and vertical extent.
pub(crate) const BAR_START: f64 = 8.0;
pub(crate) const BAR_SIZE: [f64; 2] = [24.0, 48.0];
pub(crate) const BAR_TOP: f64 = 12.0;
const BAR_VALUE: f64 = 0.5;

const CELL: f64 = 12.0;
const CHECKER: [f64; 2] = [0.3, 0.7];

/// Static plane: rows from `PLANE_TOP` for `PLANE_HEIGHT`, columns from `PLANE_LEFT` for `PLANE_WIDTH`.
pub(crate) const PLANE_TOP: f64 = 66.0;
pub(crate) const PLANE_HEIGHT: f64 = 24.0;
pub(crate) const PLANE_LEFT: f64 = 8.0;
pub(crate) const PLANE_WIDTH: f64 = 144.0;
/// Stripe phase is `PLANE_CYCLES * u^2` for the row fraction `u`, so the period shrinks from
/// infinity to `PLANE_HEIGHT / (2 * PLANE_CYCLES)` rows.
const PLANE_CYCLES: f64 = 4.0;
const PLANE_VALUES: [f64; 2] = [0.2, 0.8];

/// Device depths of the three surfaces.
pub(crate) const BAR_DEPTH: f64 = 0.4;
pub(crate) const BACKGROUND_DEPTH: f64 = 0.8;
pub(crate) const PLANE_DEPTH: [f64; 2] = [0.6, 0.8];

/// Left edge of the bar at `frame`.
pub(crate) fn bar_left(frame: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame indices of a test sequence are far below 2^52"
    )]
    let frames = frame as f64;
    BAR_START + SPEED * frames
}

fn inside(p: [f64; 2], origin: [f64; 2], size: [f64; 2]) -> bool {
    (0..2).all(|i| p[i] >= origin[i] && p[i] < origin[i] + size[i])
}

fn checker(p: [f64; 2]) -> f64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "cell indices of a 160 x 96 canvas are tiny"
    )]
    let cell = |v: f64| (v / CELL).floor() as i64;
    let parity = (cell(p[0]) + cell(p[1])).rem_euclid(2);
    CHECKER[usize::from(parity == 1)]
}

fn plane(p: [f64; 2]) -> Option<f64> {
    if !inside(p, [PLANE_LEFT, PLANE_TOP], [PLANE_WIDTH, PLANE_HEIGHT]) {
        return None;
    }
    let u = (p[1] - PLANE_TOP) / PLANE_HEIGHT;
    let phase = PLANE_CYCLES * u * u;
    Some(PLANE_VALUES[usize::from(phase.fract() >= 0.5)])
}

/// Scene-referred value at canvas position `p` when the bar's left edge is at `left`.
pub(crate) fn value(p: [f64; 2], left: f64) -> f64 {
    if inside(p, [left, BAR_TOP], BAR_SIZE) {
        BAR_VALUE
    } else {
        plane(p).unwrap_or_else(|| checker(p))
    }
}

/// WGSL that rasterises the scene at the internal extent and the jittered sample position of each
/// pixel, writing colour, backward motion in uv units and device depth. The uniform holds the
/// internal extent, the jitter in internal pixels, the bar's left edge in `bar.x` and its velocity
/// in canvas pixels per frame, +x, in `bar.y`.
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
fn within(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>) -> bool {{
    return all(p >= origin) && all(p < origin + size);
}}
@fragment fn paint(@builtin(position) frag: vec4<f32>) -> Out {{
    let p = (frag.xy - frame.jitter) / frame.render * vec2<f32>({w:?}, {h:?});
    var o: Out;
    o.motion = vec2<f32>(0.0);
    if within(p, vec2<f32>(frame.bar.x, {BAR_TOP:?}), vec2<f32>({bw:?}, {bh:?})) {{
        o.color = vec4<f32>({BAR_VALUE:?}, {BAR_VALUE:?}, {BAR_VALUE:?}, 1.0);
        o.motion = vec2<f32>(-frame.bar.y / {w:?}, 0.0);
        o.depth = {BAR_DEPTH:?};
        return o;
    }}
    var v = {c0:?};
    if ((i32(floor(p.x / {CELL:?})) + i32(floor(p.y / {CELL:?}))) & 1) == 1 {{ v = {c1:?}; }}
    o.depth = {BACKGROUND_DEPTH:?};
    if within(p, vec2<f32>({PLANE_LEFT:?}, {PLANE_TOP:?}), vec2<f32>({PLANE_WIDTH:?}, {PLANE_HEIGHT:?})) {{
        let u = (p.y - {PLANE_TOP:?}) / {PLANE_HEIGHT:?};
        v = select({p0:?}, {p1:?}, fract({PLANE_CYCLES:?} * u * u) >= 0.5);
        o.depth = {d0:?} + ({d1:?} - {d0:?}) * u;
    }}
    o.color = vec4<f32>(v, v, v, 1.0);
    return o;
}}
",
        w = CANVAS[0],
        h = CANVAS[1],
        bw = BAR_SIZE[0],
        bh = BAR_SIZE[1],
        c0 = CHECKER[0],
        c1 = CHECKER[1],
        p0 = PLANE_VALUES[0],
        p1 = PLANE_VALUES[1],
        d0 = PLANE_DEPTH[0],
        d1 = PLANE_DEPTH[1],
    )
}
