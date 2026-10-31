//! The analytic image of the temporal quality measurement. One description feeds both the f64
//! CPU function that builds the reference and the WGSL source that rasterises the internal
//! frame, so the two cannot drift apart.
//!
//! Positions are in output pixels of a fixed 160 x 96 canvas, whatever the extents under test:
//! the rasteriser maps a uv position onto the canvas, so every render scale sees the same
//! picture. The image is a smooth gradient with three high contrast features: a slanted edge, a
//! slanted line 1.2 pixels wide and a disc.

/// Canvas size in output pixels.
pub(crate) const CANVAS: [f64; 2] = [160.0, 96.0];

const BASE: f64 = 0.2;
/// Gradient rise across the canvas width and height.
const SLOPE: [f64; 2] = [0.3, 0.2];

const EDGE_ANGLE: f64 = 0.35;
const EDGE_OFFSET: f64 = 70.0;
const EDGE_STEP: f64 = 2.0;

const LINE_ANGLE: f64 = -0.6;
const LINE_OFFSET: f64 = 60.0;
const LINE_HALF_WIDTH: f64 = 0.6;
const LINE_STEP: f64 = 3.0;

const DISC_CENTRE: [f64; 2] = [118.0, 30.0];
const DISC_RADIUS: f64 = 16.0;
const DISC_STEP: f64 = 1.5;

fn signed_distance_to_line(p: [f64; 2], angle: f64, offset: f64) -> f64 {
    p[0] * angle.cos() + p[1] * angle.sin() - offset
}

fn signed_distance_to_disc(p: [f64; 2]) -> f64 {
    (p[0] - DISC_CENTRE[0]).hypot(p[1] - DISC_CENTRE[1]) - DISC_RADIUS
}

/// Scene-referred value at canvas position `p`.
pub(crate) fn value(p: [f64; 2]) -> f64 {
    let mut v = BASE + SLOPE[0] * p[0] / CANVAS[0] + SLOPE[1] * p[1] / CANVAS[1];
    if signed_distance_to_line(p, EDGE_ANGLE, EDGE_OFFSET) > 0.0 {
        v += EDGE_STEP;
    }
    if signed_distance_to_line(p, LINE_ANGLE, LINE_OFFSET).abs() < LINE_HALF_WIDTH {
        v += LINE_STEP;
    }
    if signed_distance_to_disc(p) < 0.0 {
        v += DISC_STEP;
    }
    v
}

/// Distance in output pixels from `p` to the nearest discontinuity.
pub(crate) fn edge_distance(p: [f64; 2]) -> f64 {
    let edge = signed_distance_to_line(p, EDGE_ANGLE, EDGE_OFFSET).abs();
    let line = (signed_distance_to_line(p, LINE_ANGLE, LINE_OFFSET).abs() - LINE_HALF_WIDTH).abs();
    let disc = signed_distance_to_disc(p).abs();
    edge.min(line).min(disc)
}

/// WGSL that rasterises the image at the internal extent and the jittered sample position of each
/// pixel. The uniform holds the internal extent and the jitter in internal pixels; texel `s` is
/// sampled at `(s + 0.5 - jitter) / R`, the convention of the reconstruction shader.
pub(crate) fn wgsl() -> String {
    let (edge, line) = (EDGE_ANGLE, LINE_ANGLE);
    format!(
        "
struct Frame {{ render: vec2<f32>, jitter: vec2<f32> }}
@group(0) @binding(0) var<uniform> frame: Frame;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}}
fn scene(p: vec2<f32>) -> f32 {{
    var v = {BASE:?} + {sx:?} * p.x / {w:?} + {sy:?} * p.y / {h:?};
    if dot(p, vec2<f32>({ex:?}, {ey:?})) - {EDGE_OFFSET:?} > 0.0 {{ v += {EDGE_STEP:?}; }}
    if abs(dot(p, vec2<f32>({lx:?}, {ly:?})) - {LINE_OFFSET:?}) < {LINE_HALF_WIDTH:?} {{
        v += {LINE_STEP:?};
    }}
    if length(p - vec2<f32>({cx:?}, {cy:?})) < {DISC_RADIUS:?} {{ v += {DISC_STEP:?}; }}
    return v;
}}
@fragment fn paint(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {{
    let uv = (frag.xy - frame.jitter) / frame.render;
    let v = scene(uv * vec2<f32>({w:?}, {h:?}));
    return vec4<f32>(v, v, v, 1.0);
}}
",
        sx = SLOPE[0],
        sy = SLOPE[1],
        w = CANVAS[0],
        h = CANVAS[1],
        ex = edge.cos(),
        ey = edge.sin(),
        lx = line.cos(),
        ly = line.sin(),
        cx = DISC_CENTRE[0],
        cy = DISC_CENTRE[1],
    )
}
