//! SMAA lookup textures, generated at run time.
//!
//! This is a Rust port of `Scripts/AreaTex.py` and `Scripts/SearchTex.py` from
//! <https://github.com/iryoku/smaa>. Evaluation order matches the scripts so the
//! bytes equal the reference `AreaTex.h` and `SearchTex.h`.
//!
//! Copyright (C) 2013 Jorge Jimenez (jorge@iryoku.com)
//! Copyright (C) 2013 Jose I. Echevarria (joseignacioechevarria@gmail.com)
//! Copyright (C) 2013 Belen Masia (bmasia@unizar.es)
//! Copyright (C) 2013 Fernando Navarro (fernandn@microsoft.com)
//! Copyright (C) 2013 Diego Gutierrez (diegog@unizar.es)
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! this software and associated documentation files (the "Software"), to deal in
//! the Software without restriction, including without limitation the rights to
//! use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
//! of the Software, and to permit persons to whom the Software is furnished to
//! do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in
//! all copies or substantial portions of the Software. As clarification, there
//! is no requirement that the copyright notice and permission be included in
//! binary distributions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
use std::sync::OnceLock;

/// Area texture extent in texels (RG8, two bytes per texel, row-major).
pub(crate) const AREA_EXTENT: [u32; 2] = [160, 560];
/// Search texture extent in texels (R8, row-major).
pub(crate) const SEARCH_EXTENT: [u32; 2] = [64, 16];

const AREA_WIDTH: usize = 160;
const AREA_HEIGHT: usize = 560;
/// Orthogonal patterns fill the left half; diagonal patterns the right half.
const DIAGONAL_COLUMN: usize = 80;
const ORTHO_SIZE: usize = 16;
const DIAG_SIZE: usize = 20;
const SAMPLES_DIAG: usize = 30;
const SMOOTH_MAX_DISTANCE: f64 = 32.0;

const OFFSETS_ORTHO: [f64; 7] = [0.0, -0.25, 0.25, -0.125, 0.125, -0.375, 0.375];
const OFFSETS_DIAG: [V2; 5] = [
    [0.0, 0.0],
    [0.25, -0.25],
    [-0.25, 0.25],
    [0.125, -0.125],
    [-0.125, 0.125],
];
const EDGES_ORTHO: [(usize, usize); 16] = [
    (0, 0),
    (3, 0),
    (0, 3),
    (3, 3),
    (1, 0),
    (4, 0),
    (1, 3),
    (4, 3),
    (0, 1),
    (3, 1),
    (0, 4),
    (3, 4),
    (1, 1),
    (4, 1),
    (1, 4),
    (4, 4),
];
const EDGES_DIAG: [(usize, usize); 16] = [
    (0, 0),
    (1, 0),
    (0, 2),
    (1, 2),
    (2, 0),
    (3, 0),
    (2, 2),
    (3, 2),
    (0, 1),
    (1, 1),
    (0, 3),
    (1, 3),
    (2, 1),
    (3, 1),
    (2, 3),
    (3, 3),
];

type V2 = [f64; 2];
fn add(a: V2, b: V2) -> V2 {
    [a[0] + b[0], a[1] + b[1]]
}
fn average(a: V2, b: V2) -> V2 {
    let sum = add(a, b);
    [sum[0] / 2.0, sum[1] / 2.0]
}

/// Truncating conversion of a 0..=1 area to the script's byte range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_byte(area: f64) -> u8 {
    (255.0 * area) as u8
}
fn index(x: usize, y: usize) -> usize {
    (y * AREA_WIDTH + x) * 2
}
fn exact(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("table index fits u32"))
}

/// The pattern rule of an orthogonal L, Z or U shape; levels are line heights.
#[derive(Clone, Copy)]
enum Level {
    Upper,
    Lower,
}
#[derive(Clone, Copy)]
enum Rule {
    Empty,
    HalfLeft(Level),
    HalfRight(Level),
    Smooth(Level),
    Blend(Level, Level),
    Full(Level, Level),
}
const RULES: [Rule; 16] = [
    Rule::Empty,
    Rule::HalfLeft(Level::Lower),
    Rule::HalfRight(Level::Lower),
    Rule::Smooth(Level::Lower),
    Rule::HalfLeft(Level::Upper),
    Rule::Empty,
    Rule::Blend(Level::Upper, Level::Lower),
    Rule::Full(Level::Upper, Level::Lower),
    Rule::HalfRight(Level::Upper),
    Rule::Blend(Level::Lower, Level::Upper),
    Rule::Empty,
    Rule::Full(Level::Lower, Level::Upper),
    Rule::Smooth(Level::Upper),
    Rule::Full(Level::Lower, Level::Upper),
    Rule::Full(Level::Upper, Level::Lower),
    Rule::Empty,
];

/// Area under the line `p1 -> p2` for the pixel `x..x+1`.
fn segment_area(p1: V2, p2: V2, x: f64) -> V2 {
    let d = [p2[0] - p1[0], p2[1] - p1[1]];
    let (x1, x2) = (x, x + 1.0);
    let y1 = p1[1] + d[1] * (x1 - p1[0]) / d[0];
    let y2 = p1[1] + d[1] * (x2 - p1[0]) / d[0];
    let inside = (x1 >= p1[0] && x1 < p2[0]) || (x2 > p1[0] && x2 <= p2[0]);
    if !inside {
        return [0.0, 0.0];
    }
    let trapezoid = 1f64.copysign(y1) == 1f64.copysign(y2) || y1.abs() < 1e-4 || y2.abs() < 1e-4;
    if trapezoid {
        let a = (y1 + y2) / 2.0;
        return if a < 0.0 {
            [a.abs(), 0.0]
        } else {
            [0.0, a.abs()]
        };
    }
    let crossing = -p1[1] * d[0] / d[1] + p1[0];
    let fraction = crossing.fract();
    let a1 = if crossing > p1[0] {
        y1 * fraction / 2.0
    } else {
        0.0
    };
    let a2 = if crossing < p2[0] {
        y2 * (1.0 - fraction) / 2.0
    } else {
        0.0
    };
    let a = if a1.abs() > a2.abs() { a1 } else { -a2 };
    if a < 0.0 {
        [a1.abs(), a2.abs()]
    } else {
        [a2.abs(), a1.abs()]
    }
}

/// Blends small U shapes toward a square-root profile with distance.
fn smooth_area(d: f64, a1: V2, a2: V2) -> V2 {
    let p = (d / SMOOTH_MAX_DISTANCE).clamp(0.0, 1.0);
    let smooth = |a: V2| -> V2 {
        let b = [(a[0] * 2.0).sqrt() * 0.5, (a[1] * 2.0).sqrt() * 0.5];
        [b[0] + (a[0] - b[0]) * p, b[1] + (a[1] - b[1]) * p]
    };
    add(smooth(a1), smooth(a2))
}

/// Area of an orthogonal pattern for the given distances and subsample offset.
fn area_ortho(pattern: usize, left: usize, right: usize, offset: f64) -> V2 {
    let d = exact(left + right + 1);
    let x = exact(left);
    let height = |level: Level| match level {
        Level::Upper => 0.5 + offset,
        Level::Lower => 0.5 + offset - 1.0,
    };
    let half = d / 2.0;
    match RULES[pattern] {
        Rule::Empty => [0.0, 0.0],
        Rule::HalfLeft(level) if left <= right => segment_area([0.0, height(level)], [half, 0.0], x),
        Rule::HalfRight(level) if left >= right => {
            segment_area([half, 0.0], [d, height(level)], x)
        }
        Rule::HalfLeft(_) | Rule::HalfRight(_) => [0.0, 0.0],
        Rule::Smooth(level) => {
            let a1 = segment_area([0.0, height(level)], [half, 0.0], x);
            let a2 = segment_area([half, 0.0], [d, height(level)], x);
            smooth_area(d, a1, a2)
        }
        Rule::Blend(from, to) if offset.abs() > 0.0 => {
            let (start, end) = ([0.0, height(from)], [d, height(to)]);
            let full = segment_area(start, end, x);
            let mut halves = segment_area(start, [half, 0.0], x);
            halves = add(halves, segment_area([half, 0.0], end, x));
            average(full, halves)
        }
        Rule::Blend(from, to) | Rule::Full(from, to) => {
            segment_area([0.0, height(from)], [d, height(to)], x)
        }
    }
}

/// Fraction of a 30x30 sample grid over a pixel that lies on one side of `p1 -> p2`.
fn diag_coverage(p1: V2, p2: V2, p: V2) -> f64 {
    if p1 == p2 {
        return 1.0;
    }
    let (xm, ym) = ((p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0);
    let (a, b) = (p2[1] - p1[1], p1[0] - p2[0]);
    let steps: Vec<f64> = (0..SAMPLES_DIAG)
        .map(|i| exact(i) / exact(SAMPLES_DIAG - 1))
        .collect();
    let mut inside = 0_u32;
    for sx in &steps {
        let x = p[0] + sx;
        for sy in &steps {
            let y = p[1] + sy;
            inside += u32::from(a * (x - xm) + b * (y - ym) > 0.0);
        }
    }
    f64::from(inside) / exact(SAMPLES_DIAG * SAMPLES_DIAG)
}

/// Pixel and opposite-pixel coverage for one diagonal line hypothesis.
fn diag_line(pattern: usize, ends: (V2, V2), left: f64, offset: V2) -> V2 {
    let (e1, e2) = EDGES_DIAG[pattern];
    let p1 = if e1 > 0 { add(ends.0, offset) } else { ends.0 };
    let p2 = if e2 > 0 { add(ends.1, offset) } else { ends.1 };
    let a1 = diag_coverage(p1, p2, add([1.0, 0.0], [left, left]));
    let a2 = diag_coverage(p1, p2, add([1.0, 1.0], [left, left]));
    [1.0 - a1, a2]
}

/// Start corner and end corner (before adding the span) of each pattern's hypotheses.
const DIAG_HYPOTHESES: [&[(V2, V2)]; 16] = [
    &[([1.0, 1.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 0.0], [0.0, 0.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([0.0, 0.0], [1.0, 0.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 1.0], [0.0, 0.0]), ([1.0, 1.0], [1.0, 0.0])],
    &[([1.0, 1.0], [0.0, 0.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 1.0], [1.0, 0.0])],
    &[([1.0, 1.0], [1.0, 0.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([0.0, 0.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 1.0])],
    &[([1.0, 0.0], [1.0, 1.0])],
    &[([0.0, 0.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 0.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 0.0])],
    &[([1.0, 1.0], [1.0, 1.0])],
    &[([1.0, 1.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 1.0])],
    &[([1.0, 1.0], [1.0, 1.0]), ([1.0, 1.0], [1.0, 0.0])],
    &[([1.0, 1.0], [1.0, 1.0]), ([1.0, 0.0], [1.0, 0.0])],
];

/// Area of a diagonal pattern; the unknown line ends are blended over both hypotheses.
fn area_diag(pattern: usize, left: usize, right: usize, offset: V2) -> V2 {
    let d = exact(left + right + 1);
    let span = [d, d];
    let lines: Vec<V2> = DIAG_HYPOTHESES[pattern]
        .iter()
        .map(|&(start, end)| diag_line(pattern, (start, add(end, span)), exact(left), offset))
        .collect();
    match lines.as_slice() {
        [only] => *only,
        [first, second] => average(*first, *second),
        _ => unreachable!("every diagonal pattern has one or two hypotheses"),
    }
}

fn put(texture: &mut [u8], x: usize, y: usize, area: V2) {
    let at = index(x, y);
    texture[at] = to_byte(area[0]);
    texture[at + 1] = to_byte(area[1]);
}

fn orthogonal_area_texture() -> Vec<u8> {
    let mut texture = vec![0; AREA_WIDTH * AREA_HEIGHT * 2];
    for (slot, offset) in OFFSETS_ORTHO.iter().enumerate() {
        for (pattern, edge) in EDGES_ORTHO.iter().enumerate() {
            for left in 0..ORTHO_SIZE {
                for right in 0..ORTHO_SIZE {
                    // Distances are compressed quadratically to reach further.
                    let area = area_ortho(pattern, left * left, right * right, *offset);
                    let x = left + ORTHO_SIZE * edge.0;
                    let y = 5 * ORTHO_SIZE * slot + right + ORTHO_SIZE * edge.1;
                    put(&mut texture, x, y, area);
                }
            }
        }
    }
    texture
}

fn diagonal_area_texture() -> Vec<u8> {
    let mut texture = vec![0; AREA_WIDTH * AREA_HEIGHT * 2];
    for (slot, offset) in OFFSETS_DIAG.iter().enumerate() {
        for (pattern, edge) in EDGES_DIAG.iter().enumerate() {
            for left in 0..DIAG_SIZE {
                for right in 0..DIAG_SIZE {
                    let area = area_diag(pattern, left, right, *offset);
                    let x = DIAGONAL_COLUMN + left + DIAG_SIZE * edge.0;
                    let y = 4 * DIAG_SIZE * slot + right + DIAG_SIZE * edge.1;
                    put(&mut texture, x, y, area);
                }
            }
        }
    }
    texture
}

fn orthogonal_cached() -> &'static [u8] {
    static CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    CACHE.get_or_init(orthogonal_area_texture)
}
fn diagonal_cached() -> &'static [u8] {
    static CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    CACHE.get_or_init(diagonal_area_texture)
}

/// The 160x560 RG8 area texture. The diagonal half is generated only when requested
/// and stays zero otherwise, since no shader path reads it then.
pub(crate) fn area_texture(diagonal: bool) -> Vec<u8> {
    let mut texture = orthogonal_cached().to_vec();
    if diagonal {
        for (out, diag) in texture.iter_mut().zip(diagonal_cached()) {
            *out |= *diag;
        }
    }
    texture
}

/// Which of the 16 bilinear edge combinations produces each search coordinate.
fn bilinear_pattern(texcoord: f64) -> Option<[u8; 4]> {
    (0_u8..16).find_map(|bits| {
        let e = [(bits >> 3) & 1, (bits >> 2) & 1, (bits >> 1) & 1, bits & 1];
        let value = |i: usize| f64::from(e[i]);
        let lerp = |v0: f64, v1: f64, p: f64| v0 + (v1 - v0) * p;
        let a = lerp(value(0), value(1), 1.0 - 0.25);
        let b = lerp(value(2), value(3), 1.0 - 0.25);
        (lerp(a, b, 1.0 - 0.125) == texcoord).then_some(e)
    })
}

/// Extra pixels to advance in the last step of a search to the left or right.
fn search_delta(right_side: bool, left: [u8; 4], top: [u8; 4]) -> u8 {
    let crossing = |a: usize, b: usize| left[a] != 1 && left[b] != 1;
    let first = top[3] == 1 && (!right_side || crossing(1, 3));
    let mut d = u8::from(first);
    let second_crossing = if right_side {
        crossing(0, 2)
    } else {
        crossing(1, 3)
    };
    if d == 1 && top[2] == 1 && second_crossing {
        d += 1;
    }
    d
}

/// The cropped, flipped 64x16 R8 search texture.
pub(crate) fn search_texture() -> Vec<u8> {
    let [width, height] = SEARCH_EXTENT.map(|v| usize::try_from(v).expect("extent fits usize"));
    let mut texture = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            // The scripts crop rows 17..33 of a 66x33 image, then flip vertically.
            let source_y = 32 - y;
            let (right_side, source_x) = if x < 33 { (false, x) } else { (true, x - 33) };
            let coord = |v: usize| 0.03125 * exact(v);
            if let (Some(left), Some(top)) =
                (bilinear_pattern(coord(source_x)), bilinear_pattern(coord(source_y)))
            {
                texture[y * width + x] = 127 * search_delta(right_side, left, top);
            }
        }
    }
    texture
}
