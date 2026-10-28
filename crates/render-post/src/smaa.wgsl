// SMAA 1x: edge detection, blending weight calculation and neighborhood blending.
//
// WGSL port of SMAA.hlsl from https://github.com/iryoku/smaa (SMAA 2.8 reference).
//
// Copyright (C) 2013 Jorge Jimenez (jorge@iryoku.com)
// Copyright (C) 2013 Jose I. Echevarria (joseignacioechevarria@gmail.com)
// Copyright (C) 2013 Belen Masia (bmasia@unizar.es)
// Copyright (C) 2013 Fernando Navarro (fernandn@microsoft.com)
// Copyright (C) 2013 Diego Gutierrez (diegog@unizar.es)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to
// do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software. As clarification, there
// is no requirement that the copyright notice and permission be included in
// binary distributions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
// Differences from the reference: the lookup textures are storage buffers read
// with explicit bilinear arithmetic (the pass builder has no queue to upload
// textures). The color input is Rgba16Float, so edge detection uses point loads
// and neighborhood blending uses the reference's two bilinear fetches. Edge
// decisions use sRGB-encoded values; blending is linear.
// There is no predication, reprojection or subsample indexing (SMAA 1x only).

@group(0) @binding(0) var color: texture_2d<f32>;
@group(0) @binding(1) var edges_tex: texture_2d<f32>;
@group(0) @binding(2) var<storage, read> area_data: array<u32>;
@group(0) @binding(3) var<storage, read> search_data: array<u32>;
@group(0) @binding(4) var blend_tex: texture_2d<f32>;
@group(0) @binding(5) var linear_clamp: sampler;

override THRESHOLD: f32 = 0.1;
override MAX_SEARCH_STEPS: f32 = 16.0;
override MAX_SEARCH_STEPS_DIAG: f32 = 8.0;
override DIAGONAL: bool = true;
override CORNERS: bool = true;
override CORNER_ROUNDING: f32 = 0.25;
override COLOR_EDGES: bool = false;

const LOCAL_CONTRAST_ADAPTATION_FACTOR: f32 = 2.0;
const AREATEX_MAX_DISTANCE: f32 = 16.0;
const AREATEX_MAX_DISTANCE_DIAG: f32 = 20.0;
const AREATEX_SIZE = vec2<f32>(160.0, 560.0);
const SEARCHTEX_SIZE = vec2<f32>(66.0, 33.0);
const SEARCHTEX_PACKED_SIZE = vec2<f32>(64.0, 16.0);

@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}

fn load_color(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(color, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(color)) - 1), 0);
}
fn load_blend(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(blend_tex, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(blend_tex)) - 1), 0);
}

// ---------------------------------------------------------------------------
// Pass 1: edge detection

fn srgb_encode(v: vec3<f32>) -> vec3<f32> {
    let c = clamp(v, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}
fn perceptual(p: vec2<i32>) -> vec3<f32> {
    return srgb_encode(load_color(p).rgb);
}
fn contrast(a: vec3<f32>, b: vec3<f32>) -> f32 {
    if COLOR_EDGES {
        let t = abs(a - b);
        return max(max(t.r, t.g), t.b);
    }
    let weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    return abs(dot(a, weights) - dot(b, weights));
}

// Returns (west, north) edges. Pixels without an edge stay at the cleared zero.
@fragment fn smaa_edges(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let c = perceptual(p);
    let left = perceptual(p + vec2<i32>(-1, 0));
    let top = perceptual(p + vec2<i32>(0, -1));
    let near = vec2<f32>(contrast(c, left), contrast(c, top));
    var edges = step(vec2<f32>(THRESHOLD), near);
    if dot(edges, vec2<f32>(1.0)) == 0.0 { return vec4<f32>(0.0); }

    let right = perceptual(p + vec2<i32>(1, 0));
    let bottom = perceptual(p + vec2<i32>(0, 1));
    let opposite = vec2<f32>(contrast(c, right), contrast(c, bottom));
    var max_delta = max(near, opposite);

    let leftleft = perceptual(p + vec2<i32>(-2, 0));
    let toptop = perceptual(p + vec2<i32>(0, -2));
    var far = vec2<f32>(contrast(left, leftleft), contrast(top, toptop));
    if COLOR_EDGES { far = vec2<f32>(contrast(c, leftleft), contrast(c, toptop)); }
    max_delta = max(max_delta, far);
    let final_delta = max(max_delta.x, max_delta.y);

    // Local contrast adaptation drops edges next to a much stronger one.
    edges *= step(vec2<f32>(final_delta), LOCAL_CONTRAST_ADAPTATION_FACTOR * near);
    return vec4<f32>(edges, 0.0, 0.0);
}

// ---------------------------------------------------------------------------
// Pass 2: blending weights

fn edge_texel() -> vec2<f32> {
    return 1.0 / vec2<f32>(textureDimensions(edges_tex));
}
fn sample_edges(uv: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(edges_tex, linear_clamp, uv, 0.0).rg;
}
// Integer texel offset, applied before filtering as the reference's offset overloads do.
fn sample_edges_at(uv: vec2<f32>, offset: vec2<i32>) -> vec2<f32> {
    return sample_edges(uv + vec2<f32>(offset) * edge_texel());
}

fn byte_at(words: u32, index: u32) -> f32 {
    return f32((words >> ((index & 3u) * 8u)) & 255u) / 255.0;
}
fn area_texel(p: vec2<i32>) -> vec2<f32> {
    let q = clamp(p, vec2<i32>(0), vec2<i32>(159, 559));
    let i = u32(q.y * 160 + q.x) * 2u;
    return vec2<f32>(byte_at(area_data[i >> 2u], i), byte_at(area_data[(i + 1u) >> 2u], i + 1u));
}
// Bilinear lookup at a texel-space coordinate (texel centers at integers) with
// clamped borders. Distances are square roots, so coordinates can be fractional.
fn area_lookup(texel_coord: vec2<f32>) -> vec2<f32> {
    let base = floor(texel_coord);
    let f = texel_coord - base;
    let b = vec2<i32>(base);
    let top = mix(area_texel(b), area_texel(b + vec2<i32>(1, 0)), f.x);
    let bottom = mix(area_texel(b + vec2<i32>(0, 1)), area_texel(b + vec2<i32>(1, 1)), f.x);
    return mix(top, bottom, f.y);
}
// Search coordinates land on texel centers, so the nearest texel is exact and
// immune to rounding in the edge values.
fn search_lookup(uv: vec2<f32>) -> f32 {
    let t = clamp(vec2<i32>(floor(uv * SEARCHTEX_PACKED_SIZE)), vec2<i32>(0), vec2<i32>(63, 15));
    let i = u32(t.y * 64 + t.x);
    return byte_at(search_data[i >> 2u], i);
}

fn search_length(e: vec2<f32>, offset: f32) -> f32 {
    var scale = SEARCHTEX_SIZE * vec2<f32>(0.5, -1.0);
    var bias = SEARCHTEX_SIZE * vec2<f32>(offset, 1.0);
    scale += vec2<f32>(-1.0, 1.0);
    bias += vec2<f32>(0.5, -0.5);
    scale *= 1.0 / SEARCHTEX_PACKED_SIZE;
    bias *= 1.0 / SEARCHTEX_PACKED_SIZE;
    return search_lookup(scale * e + bias);
}

fn search_x_left(start: vec2<f32>, end: f32) -> f32 {
    let texel = edge_texel();
    var uv = start;
    var e = vec2<f32>(0.0, 1.0);
    while uv.x > end && e.g > 0.8281 && e.r == 0.0 {
        e = sample_edges(uv);
        uv = uv - vec2<f32>(2.0, 0.0) * texel;
    }
    let offset = -(255.0 / 127.0) * search_length(e, 0.0) + 3.25;
    return texel.x * offset + uv.x;
}
fn search_x_right(start: vec2<f32>, end: f32) -> f32 {
    let texel = edge_texel();
    var uv = start;
    var e = vec2<f32>(0.0, 1.0);
    while uv.x < end && e.g > 0.8281 && e.r == 0.0 {
        e = sample_edges(uv);
        uv = uv + vec2<f32>(2.0, 0.0) * texel;
    }
    let offset = -(255.0 / 127.0) * search_length(e, 0.5) + 3.25;
    return -texel.x * offset + uv.x;
}
fn search_y_up(start: vec2<f32>, end: f32) -> f32 {
    let texel = edge_texel();
    var uv = start;
    var e = vec2<f32>(1.0, 0.0);
    while uv.y > end && e.r > 0.8281 && e.g == 0.0 {
        e = sample_edges(uv);
        uv = uv - vec2<f32>(0.0, 2.0) * texel;
    }
    let offset = -(255.0 / 127.0) * search_length(e.gr, 0.0) + 3.25;
    return texel.y * offset + uv.y;
}
fn search_y_down(start: vec2<f32>, end: f32) -> f32 {
    let texel = edge_texel();
    var uv = start;
    var e = vec2<f32>(1.0, 0.0);
    while uv.y < end && e.r > 0.8281 && e.g == 0.0 {
        e = sample_edges(uv);
        uv = uv + vec2<f32>(0.0, 2.0) * texel;
    }
    let offset = -(255.0 / 127.0) * search_length(e.gr, 0.5) + 3.25;
    return -texel.y * offset + uv.y;
}

fn area(dist: vec2<f32>, e1: f32, e2: f32) -> vec2<f32> {
    // Rounding prevents precision errors of bilinear filtering.
    return area_lookup(AREATEX_MAX_DISTANCE * round(4.0 * vec2<f32>(e1, e2)) + dist);
}

fn horizontal_corner(weights: vec2<f32>, tc: vec4<f32>, d: vec2<f32>) -> vec2<f32> {
    let left_right = step(d, d.yx);
    var rounding = (1.0 - CORNER_ROUNDING) * left_right;
    // Reduce blending for pixels in the center of a line.
    rounding /= left_right.x + left_right.y;
    var factor = vec2<f32>(1.0);
    factor.x -= rounding.x * sample_edges_at(tc.xy, vec2<i32>(0, 1)).r;
    factor.x -= rounding.y * sample_edges_at(tc.zw, vec2<i32>(1, 1)).r;
    factor.y -= rounding.x * sample_edges_at(tc.xy, vec2<i32>(0, -2)).r;
    factor.y -= rounding.y * sample_edges_at(tc.zw, vec2<i32>(1, -2)).r;
    return weights * saturate(factor);
}
fn vertical_corner(weights: vec2<f32>, tc: vec4<f32>, d: vec2<f32>) -> vec2<f32> {
    let left_right = step(d, d.yx);
    var rounding = (1.0 - CORNER_ROUNDING) * left_right;
    rounding /= left_right.x + left_right.y;
    var factor = vec2<f32>(1.0);
    factor.x -= rounding.x * sample_edges_at(tc.xy, vec2<i32>(1, 0)).g;
    factor.x -= rounding.y * sample_edges_at(tc.zw, vec2<i32>(1, 1)).g;
    factor.y -= rounding.x * sample_edges_at(tc.xy, vec2<i32>(-2, 0)).g;
    factor.y -= rounding.y * sample_edges_at(tc.zw, vec2<i32>(-2, 1)).g;
    return weights * saturate(factor);
}

// Bilinear fetches of two edges at a 0.25 offset encode 0.25/0.75 mixes; this unpacks them.
fn decode_diag_red(r: f32) -> f32 {
    return round(r * abs(5.0 * r - 5.0 * 0.75));
}

struct DiagSearch { dist: vec2<f32>, e: vec2<f32> }

fn search_diag1(start: vec2<f32>, dir: vec2<f32>) -> DiagSearch {
    let texel = edge_texel();
    var coord = vec4<f32>(start, -1.0, 1.0);
    var e = vec2<f32>(0.0);
    while coord.z < MAX_SEARCH_STEPS_DIAG - 1.0 && coord.w > 0.9 {
        coord = vec4<f32>(coord.xy + texel * dir, coord.z + 1.0, coord.w);
        e = sample_edges(coord.xy);
        coord.w = dot(e, vec2<f32>(0.5));
    }
    return DiagSearch(coord.zw, e);
}
fn search_diag2(start: vec2<f32>, dir: vec2<f32>) -> DiagSearch {
    let texel = edge_texel();
    var coord = vec4<f32>(start, -1.0, 1.0);
    coord.x += 0.25 * texel.x;
    var e = vec2<f32>(0.0);
    while coord.z < MAX_SEARCH_STEPS_DIAG - 1.0 && coord.w > 0.9 {
        coord = vec4<f32>(coord.xy + texel * dir, coord.z + 1.0, coord.w);
        // One bilinear fetch yields both edges; decode them.
        e = sample_edges(coord.xy);
        e = vec2<f32>(decode_diag_red(e.r), round(e.g));
        coord.w = dot(e, vec2<f32>(0.5));
    }
    return DiagSearch(coord.zw, e);
}

fn area_diag(dist: vec2<f32>, e: vec2<f32>) -> vec2<f32> {
    // Diagonal areas are on the second half of the texture.
    return area_lookup(AREATEX_MAX_DISTANCE_DIAG * e + dist + vec2<f32>(80.0, 0.0));
}

fn diag_weights(uv: vec2<f32>, e: vec2<f32>) -> vec2<f32> {
    let texel = edge_texel();
    var weights = vec2<f32>(0.0);
    var d = vec4<f32>(0.0);
    if e.r > 0.0 {
        let s = search_diag1(uv, vec2<f32>(-1.0, 1.0));
        d.x = s.dist.x + select(0.0, 1.0, s.e.g > 0.9);
        d.z = s.dist.y;
    }
    let s1 = search_diag1(uv, vec2<f32>(1.0, -1.0));
    d.y = s1.dist.x;
    d.w = s1.dist.y;

    if d.x + d.y > 2.0 {
        let coords = vec4<f32>(-d.x + 0.25, d.x, d.y, -d.y - 0.25) * texel.xyxy + uv.xyxy;
        let c1 = sample_edges_at(coords.xy, vec2<i32>(-1, 0));
        let c2 = sample_edges_at(coords.zw, vec2<i32>(1, 0));
        var cc = vec2<f32>(
            2.0 * round(c1.g) + decode_diag_red(c1.r),
            2.0 * round(c2.g) + decode_diag_red(c2.r),
        );
        // Remove the crossing edge if the end of the line was not found.
        if d.z >= 0.9 { cc.x = 0.0; }
        if d.w >= 0.9 { cc.y = 0.0; }
        weights += area_diag(d.xy, cc);
    }

    let s2 = search_diag2(uv, vec2<f32>(-1.0, -1.0));
    d.x = s2.dist.x;
    d.z = s2.dist.y;
    if sample_edges_at(uv, vec2<i32>(1, 0)).r > 0.0 {
        let s3 = search_diag2(uv, vec2<f32>(1.0, 1.0));
        d.y = s3.dist.x + select(0.0, 1.0, s3.e.g > 0.9);
        d.w = s3.dist.y;
    } else {
        d.y = 0.0;
        d.w = 0.0;
    }

    if d.x + d.y > 2.0 {
        let coords = vec4<f32>(-d.x, -d.x, d.y, d.y) * texel.xyxy + uv.xyxy;
        let c0 = sample_edges_at(coords.xy, vec2<i32>(-1, 0)).g;
        let c1 = sample_edges_at(coords.xy, vec2<i32>(0, -1)).r;
        let c23 = sample_edges_at(coords.zw, vec2<i32>(1, 0)).gr;
        var cc = vec2<f32>(2.0 * c0 + c1, 2.0 * c23.x + c23.y);
        if d.z >= 0.9 { cc.x = 0.0; }
        if d.w >= 0.9 { cc.y = 0.0; }
        weights += area_diag(d.xy, cc).gr;
    }
    return weights;
}

@fragment fn smaa_weights(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = edge_texel();
    let size = vec2<f32>(textureDimensions(edges_tex));
    let uv = pos.xy * texel;
    let offset0 = uv.xyxy + texel.xyxy * vec4<f32>(-0.25, -0.125, 1.25, -0.125);
    let offset1 = uv.xyxy + texel.xyxy * vec4<f32>(-0.125, -0.25, -0.125, 1.25);
    // Ends of the search loops.
    let offset2 = vec4<f32>(offset0.x, offset0.z, offset1.y, offset1.w)
        + texel.xxyy * vec4<f32>(-2.0, 2.0, -2.0, 2.0) * MAX_SEARCH_STEPS;

    var weights = vec4<f32>(0.0);
    var e = sample_edges(uv);

    if e.g > 0.0 {
        var horizontal = true;
        if DIAGONAL {
            // Diagonals have both north and west edges. They take priority.
            let diagonal = diag_weights(uv, e);
            weights = vec4<f32>(diagonal, weights.ba);
            if diagonal.x != -diagonal.y {
                horizontal = false;
                e.r = 0.0;
            }
        }
        if horizontal {
            let left = search_x_left(offset0.xy, offset2.x);
            let y = offset1.y;
            let e1 = sample_edges(vec2<f32>(left, y)).r;
            let right = search_x_right(offset0.zw, offset2.y);
            // Distances in pixels.
            let d = abs(round(vec2<f32>(left, right) * size.x - pos.x));
            let e2 = sample_edges_at(vec2<f32>(right, y), vec2<i32>(1, 0)).r;
            var rg = area(sqrt(d), e1, e2);
            if CORNERS { rg = horizontal_corner(rg, vec4<f32>(left, uv.y, right, uv.y), d); }
            weights = vec4<f32>(rg, weights.ba);
        }
    }

    if e.r > 0.0 {
        let top = search_y_up(offset1.xy, offset2.z);
        let x = offset0.x;
        let e1 = sample_edges(vec2<f32>(x, top)).g;
        let bottom = search_y_down(offset1.zw, offset2.w);
        let d = abs(round(vec2<f32>(top, bottom) * size.y - pos.y));
        let e2 = sample_edges_at(vec2<f32>(x, bottom), vec2<i32>(0, 1)).g;
        var ba = area(sqrt(d), e1, e2);
        if CORNERS { ba = vertical_corner(ba, vec4<f32>(uv.x, top, uv.x, bottom), d); }
        weights = vec4<f32>(weights.rg, ba);
    }
    return weights;
}

// ---------------------------------------------------------------------------
// Pass 3: neighborhood blending

@fragment fn smaa_blend(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let here = load_blend(p);
    // Right, bottom, left and top weights of this pixel's edges.
    let a = vec4<f32>(load_blend(p + vec2<i32>(1, 0)).a, load_blend(p + vec2<i32>(0, 1)).g, here.z, here.x);
    let c = load_color(p);
    if dot(a, vec4<f32>(1.0)) < 1e-5 { return c; }

    // Blend along the direction with the largest weight.
    let h = max(a.x, a.z) > max(a.y, a.w);
    var amount = a.yw;
    var step_a = vec2<i32>(0, 1);
    var step_b = vec2<i32>(0, -1);
    if h {
        amount = a.xz;
        step_a = vec2<i32>(1, 0);
        step_b = vec2<i32>(-1, 0);
    }
    let weight = amount / dot(amount, vec2<f32>(1.0));
    // One bilinear fetch per side, as in the reference: sampling `amount` texels
    // along the axis mixes this pixel with its neighbor by that fraction.
    let dims = vec2<f32>(textureDimensions(color));
    let toward_a = (pos.xy + vec2<f32>(step_a) * amount.x) / dims;
    let toward_b = (pos.xy + vec2<f32>(step_b) * amount.y) / dims;
    return weight.x * textureSampleLevel(color, linear_clamp, toward_a, 0.0)
        + weight.y * textureSampleLevel(color, linear_clamp, toward_b, 0.0);
}
