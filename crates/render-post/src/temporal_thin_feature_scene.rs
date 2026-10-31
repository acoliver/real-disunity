//! The analytic thin-feature scenes of the temporal lock measurement. One description feeds both
//! the f64 CPU function that builds the reference and the WGSL source that rasterises the
//! internal frame, so the two cannot drift apart.
//!
//! Positions are in output pixels of a fixed 160 x 96 canvas, whatever the extents under test. A
//! smooth gradient carries bright lines of `HALF_WIDTH` 0.175 pixels (a 0.35 pixel wire, thinner
//! than one internal pixel at every scale) or periodic families of them (a masked line grid). The
//! content pans along +x by a speed in canvas pixels per frame; the scene shader reads the total
//! pan, the per frame step and a line gain (0 removes every line) from the `tail` of the rig.

use std::fmt::Write;

/// Canvas size in output pixels.
pub(crate) const CANVAS: [f64; 2] = [160.0, 96.0];

const BASE: f64 = 0.2;
const SLOPE: [f64; 2] = [0.3, 0.2];
/// Radiance a line adds on top of the gradient.
pub(crate) const LINE_STEP: f64 = 2.0;
const HALF_WIDTH: f64 = 0.175;
/// Distance from a line centre that belongs to the region of interest.
pub(crate) const ROI_BAND: f64 = 2.0;
/// A point every line family passes near, off the pixel grid so no line sits on a pixel centre.
const ANCHOR: [f64; 2] = [80.37, 48.21];
const GRID_PERIOD: f64 = 8.0;

/// Lines of one direction: `angle` is the direction of the lines in radians, `period` the spacing
/// of the family (0 for a single wire).
#[derive(Clone, Copy)]
struct Family {
    angle: f64,
    period: f64,
}

impl Family {
    fn normal(self) -> [f64; 2] {
        [-self.angle.sin(), self.angle.cos()]
    }

    fn offset(self) -> f64 {
        let n = self.normal();
        ANCHOR[0] * n[0] + ANCHOR[1] * n[1]
    }

    /// Distance from `p` to the centre of the nearest line of the family.
    fn distance(self, p: [f64; 2]) -> f64 {
        let n = self.normal();
        let t = p[0] * n[0] + p[1] * n[1] - self.offset();
        if self.period == 0.0 {
            t.abs()
        } else {
            (t - self.period * (t / self.period).round()).abs()
        }
    }
}

/// A named set of line families.
pub(crate) struct Fixture {
    pub(crate) name: &'static str,
    families: Vec<Family>,
}

fn wire(name: &'static str, degrees: f64) -> Fixture {
    Fixture {
        name,
        families: vec![Family {
            angle: degrees.to_radians(),
            period: 0.0,
        }],
    }
}

/// Subpixel wires at 0, 30, 45 and 90 degrees and a masked line grid.
pub(crate) fn fixtures() -> Vec<Fixture> {
    let grid = |angle: f64| Family {
        angle,
        period: GRID_PERIOD,
    };
    vec![
        wire("wire 0", 0.0),
        wire("wire 30", 30.0),
        wire("wire 45", 45.0),
        wire("wire 90", 90.0),
        Fixture {
            name: "grid",
            families: vec![grid(0.0), grid(std::f64::consts::FRAC_PI_2)],
        },
    ]
}

impl Fixture {
    /// Distance from `p` to the centre of the nearest line.
    pub(crate) fn line_distance(&self, p: [f64; 2]) -> f64 {
        self.families
            .iter()
            .map(|f| f.distance(p))
            .fold(f64::INFINITY, f64::min)
    }

    /// Scene-referred value at canvas position `p` with the lines scaled by `gain`.
    pub(crate) fn value(&self, p: [f64; 2], gain: f64) -> f64 {
        let mut v = BASE + SLOPE[0] * p[0] / CANVAS[0] + SLOPE[1] * p[1] / CANVAS[1];
        if self.line_distance(p) < HALF_WIDTH {
            v += gain * LINE_STEP;
        }
        v
    }

    /// WGSL that rasterises the scene at the internal extent and the jittered sample position of
    /// each pixel, writing colour, backward motion in uv units and a constant device depth.
    /// `tail.x` is the total pan, `tail.y` the pan of this frame and `tail.z` the line gain.
    pub(crate) fn wgsl(&self) -> String {
        let mut hits = String::new();
        for f in &self.families {
            let n = f.normal();
            let t = format!(
                "dot(p, vec2<f32>({:?}, {:?})) - {:?}",
                n[0],
                n[1],
                f.offset()
            );
            let d = if f.period == 0.0 {
                format!("abs({t})")
            } else {
                let period = f.period;
                format!("abs(({t}) - {period:?} * round(({t}) / {period:?}))")
            };
            writeln!(hits, "    if {d} < {HALF_WIDTH:?} {{ hit = true; }}").unwrap();
        }
        format!(
            "
struct Frame {{ render: vec2<f32>, jitter: vec2<f32>, tail: vec4<f32> }}
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
    let p = (frag.xy - frame.jitter) / frame.render * vec2<f32>({w:?}, {h:?})
        + vec2<f32>(frame.tail.x, 0.0);
    var hit = false;
{hits}    var v = {BASE:?} + {sx:?} * p.x / {w:?} + {sy:?} * p.y / {h:?};
    if hit {{ v += frame.tail.z * {LINE_STEP:?}; }}
    var o: Out;
    o.color = vec4<f32>(v, v, v, 1.0);
    o.motion = vec2<f32>(frame.tail.y / {w:?}, 0.0);
    o.depth = 0.5;
    return o;
}}
",
            w = CANVAS[0],
            h = CANVAS[1],
            sx = SLOPE[0],
            sy = SLOPE[1],
        )
    }
}
