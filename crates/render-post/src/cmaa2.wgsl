// CMAA2 (Conservative Morphological Anti-Aliasing 2.3), single-sample path.
//
// THIS FILE WAS MODIFIED: it is a port of CMAA2.hlsl to WGSL, made by Real Disunity contributors.
// It is not the original Intel work. The structure, constants and heuristics follow the original;
// see crates/render-post/README.md for the list of deviations. The full license text is in
// crates/render-post/LICENSE-APACHE-2.0-CMAA2.txt.
//
// Original work: Conservative Morphological Anti-Aliasing, version 2.3
// Author: Filip Strugar (filip.strugar@intel.com)
// Source: https://github.com/GameTechDev/CMAA2 (Projects/CMAA2/CMAA2/CMAA2.hlsl)
//
// Copyright (c) 2018, Intel Corporation
//
// Licensed under the Apache License, Version 2.0 ( the "License" );
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//
// Deviations from the reference (single-sample path only; the MSAA path is not ported):
// * Edges are stored one u32 per 2x2 quad (4 bits per pixel) in a storage buffer, because WGSL
//   has no R8Uint storage texture. Candidate, location and blend-item lists are storage
//   buffers addressed by linear pixel or quad index, and the quad list heads are an atomic
//   buffer, because storage textures have no InterlockedExchange. Positions are not packed
//   into 14 bits, so extents are not limited to 16384.
// * ComputeDispatchArgsCS is a one-thread entry point that writes the indirect dispatch
//   arguments of the candidate pass from its counter. Long lists wrap to further rows of 4096
//   workgroups. The control counters are cleared by a buffer clear before the edge pass instead
//   of by ComputeDispatchArgsCS.
// * The reference writes blended colors into the input image with a UAV, in a compute pass
//   (DeferredColorApply2x2CS) that visits each touched quad. Here the final fullscreen pass
//   does that work for its own pixel: it reads the pixel, and when its quad received blend
//   items it averages the items for that pixel (with the alpha of the original pixel) instead
//   of copying it. No working copy of the image and no apply pass or location list is needed,
//   and the output keeps the display-referred image's format conversion.
// * Samples outside the image are clamped to the border instead of reading as zero.
// * List overflow drops the work item instead of writing past the end of a list.
// * ComputeSimpleShapeBlendValues is ported with dontTestShapeValidity = true, the only way
//   the reference calls it. Luma edge detection (CMAA2_EDGE_DETECTION_LUMA_PATH 1) and
//   CMAA2_COLLECT_EXPAND_BLEND_ITEMS, CMAA2_DEFERRED_APPLY_THREADGROUP_SWAP are enabled.
//   Colors are packed with the R11G11B10 E4 format (non-HDR range), clamped to [0, 1.99].
// * Quality presets and CMAA2_EXTRA_SHARPNESS are pipeline-overridable constants.

override THRESHOLD: f32 = 0.07;
override EXTRA_SHARPNESS: bool = false;

@group(0) @binding(0) var color_in: texture_2d<f32>;
@group(0) @binding(3) var<storage, read_write> edge_store: array<u32>;
// Counters: 0 shape candidates, 1 deferred blend items.
@group(0) @binding(4) var<storage, read_write> control: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> candidates: array<u32>;
@group(0) @binding(6) var<storage, read_write> heads: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> items: array<vec2<u32>>;
// Indirect dispatch arguments: x, y, z of the candidate pass.
@group(0) @binding(9) var<storage, read_write> dispatch_args: array<u32>;
// Read-only views of the heads and items for the fragment stage, which cannot bind
// read-write storage.
@group(0) @binding(10) var<storage, read> heads_view: array<u32>;
@group(0) @binding(11) var<storage, read> items_view: array<vec2<u32>>;

const NO_ITEM: u32 = 0xFFFFFFFFu;
const COUNT_CANDIDATES: u32 = 0u;
const COUNT_ITEMS: u32 = 1u;
const MAX_LINE_LENGTH: f32 = 86.0;
const SYMMETRY_CORRECTION: f32 = 0.22;
const EDGE_KERNEL: u32 = 16u;
const CANDIDATE_THREADS: u32 = 128u;
const MAX_ITEMS_PER_QUAD: u32 = 32u;
const BLEND_ITEM_SLOTS: u32 = 768u;
// Workgroups per row of an indirect dispatch; longer lists wrap to further rows, which keeps
// every dimension within the device limit at any extent.
const DISPATCH_ROW: u32 = 4096u;

fn local_contrast_amount() -> f32 { return select(0.10, 0.15, EXTRA_SHARPNESS); }
fn simple_shape_blur() -> f32 { return select(0.10, 0.07, EXTRA_SHARPNESS); }
fn dampening_effect() -> f32 { return select(0.15, 0.11, EXTRA_SHARPNESS); }

@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// Image access and encodings

fn image_size() -> vec2<i32> { return vec2<i32>(textureDimensions(color_in)); }
fn quads_wide() -> u32 { return (u32(image_size().x) + 1u) / 2u; }
fn quads_high() -> u32 { return (u32(image_size().y) + 1u) / 2u; }
fn in_image(p: vec2<i32>) -> bool {
    let size = image_size();
    return p.x >= 0 && p.y >= 0 && p.x < size.x && p.y < size.y;
}
fn pixel_index(p: vec2<i32>) -> u32 { return u32(p.y) * u32(image_size().x) + u32(p.x); }
fn pixel_at(index: u32) -> vec2<i32> {
    let width = u32(image_size().x);
    return vec2<i32>(i32(index % width), i32(index / width));
}
fn load_rgba(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(color_in, clamp(p, vec2<i32>(0), image_size() - vec2<i32>(1)), 0);
}
fn load_rgb(p: vec2<i32>) -> vec3<f32> { return load_rgba(p).rgb; }

// Edge bits of a pixel: 1 right, 2 bottom, 4 left, 8 top.
fn pack_edges(edges: vec4<f32>) -> u32 { return u32(dot(edges, vec4<f32>(1.0, 2.0, 4.0, 8.0))); }
fn unpack_edges(value: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(value & 1u), f32((value >> 1u) & 1u), f32((value >> 2u) & 1u), f32((value >> 3u) & 1u),
    );
}
fn load_edge(p: vec2<i32>) -> u32 {
    let quad = vec2<i32>(i32(quads_wide()), i32(quads_high()));
    if p.x < 0 || p.y < 0 || p.x >= quad.x * 2 || p.y >= quad.y * 2 {
        return 0u;
    }
    let q = vec2<u32>(p) / 2u;
    let shift = ((u32(p.y) & 1u) * 2u + (u32(p.x) & 1u)) * 4u;
    return (edge_store[q.y * quads_wide() + q.x] >> shift) & 0xFu;
}

fn half_bits(x: f32) -> u32 { return pack2x16float(vec2<f32>(x, 0.0)) & 0xFFFFu; }
fn half_value(bits: u32) -> f32 { return unpack2x16float(bits & 0xFFFFu).x; }
// R11G11B10 E4 float packing for display range colors (Pack_R11G11B10_E4_FLOAT).
fn pack_color(color: vec3<f32>) -> u32 {
    let c = clamp(color, vec3<f32>(0.0), vec3<f32>(1.99));
    let r = ((half_bits(c.x) + 4u) >> 3u) & 0x7FFu;
    let g = ((half_bits(c.y) + 4u) << 8u) & 0x3FF800u;
    let b = ((half_bits(c.z) + 8u) << 18u) & 0xFFC00000u;
    return r | g | b;
}
fn unpack_color(packed: u32) -> vec3<f32> {
    return vec3<f32>(
        half_value((packed << 3u) & 0x3FF8u),
        half_value((packed >> 8u) & 0x3FF8u),
        half_value((packed >> 18u) & 0x3FF0u),
    );
}

// Queues one blended color for the pixel's 2x2 quad (StoreColorSample).
fn store_color_sample(pixel: vec2<i32>, color: vec3<f32>, complex_shape: bool) {
    if !in_image(pixel) { return; }
    let item = atomicAdd(&control[COUNT_ITEMS], 1u);
    if item >= arrayLength(&items) { return; }
    let quad = vec2<u32>(pixel) / 2u;
    let quad_index = quad.y * quads_wide() + quad.x;
    let offset_xy = (u32(pixel.y) & 1u) * 2u + (u32(pixel.x) & 1u);
    let header = item | (offset_xy << 30u) | (select(0u, 1u, complex_shape) << 29u);
    let previous = atomicExchange(&heads[quad_index], header);
    items[item] = vec2<u32>(previous, pack_color(color));
}

// ---------------------------------------------------------------------------
// Pass 1: edge detection (EdgesColor2x2CS)

var<workgroup> frac_edges_h: array<vec4<f32>, 256>;
var<workgroup> frac_edges_v: array<vec4<f32>, 256>;
var<private> lumas: array<f32, 8>;
var<private> quad_edges: array<vec2<f32>, 4>;
var<private> hood: array<vec2<f32>, 16>;
var<private> confirmed: array<vec4<f32>, 4>;

fn edge_luma(rgb: vec3<f32>) -> f32 {
    return dot(sqrt(max(rgb, vec3<f32>(0.0))), vec3<f32>(0.299, 0.587, 0.114));
}
fn edge_pair(x: i32, y: i32) -> vec2<f32> {
    let here = lumas[x + y * 3];
    return vec2<f32>(abs(here - lumas[x + 1 + y * 3]), abs(here - lumas[x + (y + 1) * 3]));
}
fn shared_edges(addr: u32, k: u32) -> vec2<f32> {
    return vec2<f32>(frac_edges_v[addr][k], frac_edges_h[addr][k]);
}
fn neighbour(x: i32, y: i32) -> vec2<f32> { return hood[x * 4 + y]; }
fn contrast_v(x: i32, y: i32) -> f32 {
    return max(
        max(neighbour(x + 1, y).y, neighbour(x + 1, y + 1).y),
        max(neighbour(x + 2, y).y, neighbour(x + 2, y + 1).y),
    ) * local_contrast_amount();
}
fn contrast_h(x: i32, y: i32) -> f32 {
    return max(
        max(neighbour(x, y + 1).x, neighbour(x + 1, y + 1).x),
        max(neighbour(x, y + 2).x, neighbour(x + 1, y + 2).x),
    ) * local_contrast_amount();
}
fn above_threshold(value: f32) -> f32 { return select(0.0, 1.0, value > THRESHOLD); }

// Gathers the 4x4 edge neighbourhood of a quad from workgroup memory.
fn load_neighbourhood(center: u32, top_row: vec2<f32>, left_column: vec2<f32>) {
    let row = EDGE_KERNEL;
    hood[0] = shared_edges(center - row - 1u, 3u);
    hood[4] = shared_edges(center - row, 2u);
    hood[8] = shared_edges(center - row, 3u);
    hood[12] = shared_edges(center - row + 1u, 2u);
    hood[1] = shared_edges(center - 1u, 1u);
    hood[2] = shared_edges(center - 1u, 3u);
    hood[13] = shared_edges(center + 1u, 0u);
    hood[14] = shared_edges(center + 1u, 2u);
    hood[3] = shared_edges(center - 1u + row, 1u);
    hood[7] = shared_edges(center + row, 0u);
    hood[11] = shared_edges(center + row, 1u);
    hood[4].y = top_row.x;
    hood[8].y = top_row.y;
    hood[1].x = left_column.x;
    hood[2].x = left_column.y;
    hood[5] = quad_edges[0];
    hood[9] = quad_edges[1];
    hood[6] = quad_edges[2];
    hood[10] = quad_edges[3];
}

// Edge flags of the four pixels of a quad after local contrast adaptation.
fn confirm_edges(top_row: vec2<f32>, left_column: vec2<f32>) {
    let top = vec2<f32>(
        above_threshold(top_row.x - contrast_h(0, -1)),
        above_threshold(top_row.y - contrast_h(1, -1)),
    );
    let left = vec2<f32>(
        above_threshold(left_column.x - contrast_v(-1, 0)),
        above_threshold(left_column.y - contrast_v(-1, 1)),
    );
    var own: array<vec2<f32>, 4>;
    for (var i = 0; i < 4; i++) {
        let x = i & 1;
        let y = i >> 1;
        own[i] = vec2<f32>(
            above_threshold(quad_edges[i].x - contrast_v(x, y)),
            above_threshold(quad_edges[i].y - contrast_h(x, y)),
        );
    }
    confirmed[0] = vec4<f32>(own[0], left.x, top.x);
    confirmed[1] = vec4<f32>(own[1], own[0].x, top.y);
    confirmed[2] = vec4<f32>(own[2], left.y, own[0].y);
    confirmed[3] = vec4<f32>(own[3], own[2].x, own[1].y);
}

// Appends shape candidates and returns the packed edges of the quad.
fn collect_candidates(pixel: vec2<i32>) -> u32 {
    var packed = 0u;
    for (var i = 0u; i < 4u; i++) {
        let e = confirmed[i];
        let corner = e.x * e.y + e.y * e.z + e.z * e.w + e.w * e.x;
        let at = pixel + vec2<i32>(i32(i & 1u), i32(i >> 1u));
        if corner != 0.0 && in_image(at) {
            let slot = atomicAdd(&control[COUNT_CANDIDATES], 1u);
            if slot < arrayLength(&candidates) {
                candidates[slot] = pixel_index(at);
            }
        }
        packed |= pack_edges(e) << (4u * i);
    }
    return packed;
}

fn process_quad(pixel: vec2<i32>, center: u32) {
    let quad = vec2<u32>(pixel / 2);
    if quad.x >= quads_wide() || quad.y >= quads_high() { return; }
    let top_row = vec2<f32>(frac_edges_h[center - EDGE_KERNEL].z, frac_edges_h[center - EDGE_KERNEL].w);
    let left_column = vec2<f32>(frac_edges_v[center - 1u].y, frac_edges_v[center - 1u].w);
    let total = quad_edges[0] + quad_edges[1] + quad_edges[2] + quad_edges[3] + top_row + left_column;
    var packed = 0u;
    let quad_index = quad.y * quads_wide() + quad.x;
    // Every quad starts empty, so the final pass never reads items of an earlier frame.
    atomicStore(&heads[quad_index], NO_ITEM);
    if total.x + total.y > 0.0 {
        load_neighbourhood(center, top_row, left_column);
        confirm_edges(top_row, left_column);
        packed = collect_candidates(pixel);
    }
    // All edges are written, empty ones too, so stale edges of a previous frame never survive.
    edge_store[quad_index] = packed;
}

@compute @workgroup_size(16, 16, 1)
fn cmaa2_edges(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
) {
    let pixel = (vec2<i32>(group.xy) * 14 + vec2<i32>(local.xy) - vec2<i32>(1)) * 2;
    let center = local.x + local.y * EDGE_KERNEL;
    let in_kernel = local.x != 0u && local.x != EDGE_KERNEL - 1u
        && local.y != 0u && local.y != EDGE_KERNEL - 1u;
    for (var i = 0; i < 8; i++) {
        lumas[i] = edge_luma(load_rgb(pixel + vec2<i32>(i % 3, i / 3)));
    }
    quad_edges[0] = edge_pair(0, 0);
    quad_edges[1] = edge_pair(1, 0);
    quad_edges[2] = edge_pair(0, 1);
    quad_edges[3] = edge_pair(1, 1);
    frac_edges_v[center] = vec4<f32>(quad_edges[0].x, quad_edges[1].x, quad_edges[2].x, quad_edges[3].x);
    frac_edges_h[center] = vec4<f32>(quad_edges[0].y, quad_edges[1].y, quad_edges[2].y, quad_edges[3].y);
    workgroupBarrier();
    if in_kernel {
        process_quad(pixel, center);
    }
}

// ---------------------------------------------------------------------------
// Dispatch arguments (ComputeDispatchArgsCS)

@compute @workgroup_size(1, 1, 1)
fn cmaa2_candidate_args() {
    let count = min(atomicLoad(&control[COUNT_CANDIDATES]), arrayLength(&candidates));
    let groups = (count + CANDIDATE_THREADS - 1u) / CANDIDATE_THREADS;
    let x = min(groups, DISPATCH_ROW);
    dispatch_args[0] = x;
    dispatch_args[1] = select(1u, (groups + x - 1u) / max(x, 1u), groups > 0u);
    dispatch_args[2] = 1u;
}

// ---------------------------------------------------------------------------
// Pass 2: shape candidates (ProcessCandidatesCS)

var<workgroup> blend_item_count: atomic<u32>;
var<workgroup> blend_items: array<vec2<u32>, 768>;

// Blend weights from left, above, right and below for an L-like or isolated corner pixel.
fn simple_shape_blend(
    edges: vec4<f32>, left: vec4<f32>, right: vec4<f32>, top: vec4<f32>, bottom: vec4<f32>,
) -> vec4<f32> {
    var from_right = edges.r;
    var from_below = edges.g;
    var from_left = edges.b;
    var from_above = edges.a;
    var blur = simple_shape_blur();
    let edge_count = dot(edges, vec4<f32>(1.0));
    let edges_around = dot(left.bga + right.rga + top.rba + bottom.rgb, vec3<f32>(1.0));
    if edge_count == 2.0 {
        blur *= 0.75;
        let k = 0.9;
        from_right += k * (edges.g * top.r * (1.0 - left.g) + edges.a * bottom.r * (1.0 - left.a));
        from_below += k * (edges.b * right.g * (1.0 - top.b) + edges.r * left.g * (1.0 - top.r));
        from_left += k * (edges.a * bottom.b * (1.0 - right.a) + edges.g * top.b * (1.0 - right.g));
        from_above += k * (edges.r * left.a * (1.0 - bottom.r) + edges.b * right.a * (1.0 - bottom.b));
    }
    // Dampen the blur when there are lots of neighbouring edges: this preserves text and texture.
    if EXTRA_SHARPNESS {
        blur *= saturate(1.15 - edges_around / 8.0);
    } else {
        blur *= saturate(1.30 - edges_around / 10.0);
    }
    return vec4<f32>(from_left, from_above, from_right, from_below) * blur;
}

// Scores of the inverted and normal Z shapes, as (inverted, normal).
fn detect_z(e: vec4<f32>, m1: vec4<f32>, p1: vec4<f32>, p2: vec4<f32>) -> vec2<f32> {
    let inverted = e.r * e.g * p1.a
        * (2.0 + (m1.g + p2.a) - (e.a + p1.g) - 0.7 * (p2.g + m1.a + e.b + p1.r));
    let normal = e.r * e.a * p1.g
        * (2.0 + (m1.a + p2.g) - (e.g + p1.a) - 0.7 * (p2.a + m1.g + e.b + p1.r));
    return vec2<f32>(inverted, normal);
}

// Lengths of the edge runs on each side of a Z shape's centre, as (left, right).
fn find_z_line_lengths(
    pixel: vec2<i32>, horizontal: bool, inverted: bool, step_right: vec2<i32>,
) -> vec2<f32> {
    var mask_left = select(0x04u, 0x08u, horizontal);
    var mask_right = select(0x01u, 0x02u, horizontal);
    if inverted {
        let swap = mask_left;
        mask_left = mask_right;
        mask_right = swap;
    }
    var continue_left = true;
    var continue_right = true;
    var length_left = 1.0;
    var length_right = 1.0;
    let growth = select(1.25, 1.20, EXTRA_SHARPNESS);
    let offset = select(0.25, 0.20, EXTRA_SHARPNESS);
    loop {
        let edge_left = load_edge(pixel - step_right * i32(length_left));
        let edge_right = load_edge(pixel + step_right * (i32(length_right) + 1));
        continue_left = continue_left && (edge_left & mask_left) == mask_left;
        continue_right = continue_right && (edge_right & mask_right) == mask_right;
        length_left += f32(continue_left);
        length_right += f32(continue_right);
        var longest = max(length_left, length_right);
        if !continue_left && !continue_right {
            longest = MAX_LINE_LENGTH;
        }
        if longest >= min(MAX_LINE_LENGTH, growth * min(length_right, length_left) - offset) {
            break;
        }
    }
    return vec2<f32>(length_left, length_right);
}

struct ZShape {
    pixel: vec2<i32>,
    horizontal: bool,
    inverted: bool,
    quality: f32,
    lengths: vec2<f32>,
}

fn z_step_right(z: ZShape) -> vec2<i32> { return select(vec2<i32>(0, -1), vec2<i32>(1, 0), z.horizontal); }
fn z_blend_dir(z: ZShape) -> vec2<i32> {
    let dir = select(vec2<i32>(-1, 0), vec2<i32>(0, -1), z.horizontal);
    return select(dir, -dir, z.inverted);
}

// One blend along the line: the pixel is mixed toward the neighbour across the line.
fn z_blend_color(start: vec2<i32>, z: ZShape, step_index: f32, src_offset: f32, lerp_k: f32) -> vec3<f32> {
    let at = start + z_step_right(z) * i32(step_index);
    let center = load_rgb(at);
    let across = load_rgb(at + z_blend_dir(z) * i32(src_offset));
    return mix(center, across, lerp_k);
}

struct ZRamp {
    first: f32,
    last: f32,
    spacing: f32,
    first_k: f32,
    damping: f32,
}

fn z_ramp(z: ZShape) -> ZRamp {
    let left_odd = SYMMETRY_CORRECTION * (z.lengths.x % 2.0);
    let right_odd = SYMMETRY_CORRECTION * (z.lengths.y % 2.0);
    let first = -floor((z.lengths.x + 1.0) / 2.0) + 1.0;
    let last = floor((z.lengths.y + 1.0) / 2.0);
    let total = (last - first) + 1.0 - left_odd - right_odd;
    let spacing = 1.0 / total;
    let damping = saturate((z.lengths.x + z.lengths.y - z.quality) * dampening_effect());
    return ZRamp(first, last, spacing, (0.5 - left_odd - first) * spacing, damping);
}

// Source offset (-1 before the centre, +1 after) and interpolation weight at a ramp position.
fn z_ramp_at(ramp: ZRamp, i: f32) -> vec2<f32> {
    let second = select(0.0, 1.0, i > 0.0);
    let src_offset = 1.0 - second * 2.0;
    let lerp_k = (ramp.spacing * i + ramp.first_k) * src_offset + second;
    return vec2<f32>(src_offset, lerp_k * ramp.damping);
}

// Queues the blend of a Z shape in workgroup memory, to be expanded across all threads.
// Returns false when the workgroup list is full.
fn collect_blend_zs(z: ZShape) -> bool {
    let ramp = z_ramp(z);
    let count = u32(ramp.last - ramp.first + 1.0);
    var slot = atomicAdd(&blend_item_count, count);
    if slot + count > BLEND_ITEM_SLOTS { return false; }
    let static_bits = (select(0u, 1u, z.horizontal) << 31u) | (select(0u, 1u, z.inverted) << 30u);
    for (var i = ramp.first; i <= ramp.last; i += 1.0) {
        let at = z_ramp_at(ramp, i);
        blend_items[slot] = vec2<u32>(
            pixel_index(z.pixel),
            static_bits | (u32(i + 256.0) << 20u) | (u32(at.x + 256.0) << 10u)
                | u32(saturate(at.y) * 1023.0 + 0.5),
        );
        slot += 1u;
    }
    return true;
}

// Fallback for a full workgroup list: blends in place.
fn blend_zs(z: ZShape) {
    let ramp = z_ramp(z);
    for (var i = ramp.first; i <= ramp.last; i += 1.0) {
        let at = z_ramp_at(ramp, i);
        let color = z_blend_color(z.pixel, z, i, at.x, at.y);
        store_color_sample(z.pixel + z_step_right(z) * i32(i), color, true);
    }
}

fn blend_simple_shape(pixel: vec2<i32>, blend: vec4<f32>) {
    var color = load_rgb(pixel) * (1.0 - dot(blend, vec4<f32>(1.0)));
    if blend.x > 0.0 { color += blend.x * load_rgb(pixel + vec2<i32>(-1, 0)); }
    if blend.y > 0.0 { color += blend.y * load_rgb(pixel + vec2<i32>(0, -1)); }
    if blend.z > 0.0 { color += blend.z * load_rgb(pixel + vec2<i32>(1, 0)); }
    if blend.w > 0.0 { color += blend.w * load_rgb(pixel + vec2<i32>(0, 1)); }
    store_color_sample(pixel, color, false);
}

fn quality_score(max_score: f32) -> f32 {
    let raw = clamp(4.0 - max_score, 0.0, 3.0);
    return select(floor(raw), floor(raw + 0.5), EXTRA_SHARPNESS);
}

fn process_candidate(pixel: vec2<i32>) {
    let edges = unpack_edges(load_edge(pixel));
    let left = unpack_edges(load_edge(pixel + vec2<i32>(-1, 0)));
    let right = unpack_edges(load_edge(pixel + vec2<i32>(1, 0)));
    let bottom = unpack_edges(load_edge(pixel + vec2<i32>(0, 1)));
    let top = unpack_edges(load_edge(pixel + vec2<i32>(0, -1)));
    blend_simple_shape(pixel, simple_shape_blend(edges, left, right, top, bottom));

    // Complex shapes. The vertical case reuses the horizontal one with the edges rotated by .wxyz.
    let across = detect_z(edges, left, right, unpack_edges(load_edge(pixel + vec2<i32>(2, 0))));
    let down = detect_z(
        edges.wxyz, bottom.wxyz, top.wxyz, unpack_edges(load_edge(pixel + vec2<i32>(0, -2))).wxyz,
    );
    var score = max(across.x, across.y);
    var horizontal = true;
    var inverted = across.x > across.y;
    if max(down.x, down.y) > score {
        score = max(down.x, down.y);
        horizontal = false;
        inverted = down.x > down.y;
    }
    if score <= 0.0 { return; }
    var z = ZShape(pixel, horizontal, inverted, quality_score(score), vec2<f32>(0.0));
    z.lengths = find_z_line_lengths(pixel, horizontal, inverted, z_step_right(z)) - vec2<f32>(z.quality);
    if z.lengths.x + z.lengths.y >= 5.0 && !collect_blend_zs(z) {
        blend_zs(z);
    }
}

// Applies the Z blends queued in workgroup memory, spread over every thread of the group.
fn expand_blend_items(local: u32) {
    let total = min(BLEND_ITEM_SLOTS, atomicLoad(&blend_item_count));
    let loops = (total + (CANDIDATE_THREADS - 1u) - local) / CANDIDATE_THREADS;
    for (var turn = 0u; turn < loops; turn++) {
        let item = blend_items[turn * CANDIDATE_THREADS + local];
        let horizontal = ((item.y >> 31u) & 1u) != 0u;
        let inverted = ((item.y >> 30u) & 1u) != 0u;
        let step_index = f32((item.y >> 20u) & 0x3FFu) - 256.0;
        let src_offset = f32((item.y >> 10u) & 0x3FFu) - 256.0;
        let lerp_k = f32(item.y & 0x3FFu) / 1023.0;
        let start = pixel_at(item.x);
        let z = ZShape(start, horizontal, inverted, 0.0, vec2<f32>(0.0));
        let color = z_blend_color(start, z, step_index, src_offset, lerp_k);
        store_color_sample(start + z_step_right(z) * i32(step_index), color, true);
    }
}

@compute @workgroup_size(128, 1, 1)
fn cmaa2_candidates(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    if local == 0u {
        atomicStore(&blend_item_count, 0u);
    }
    workgroupBarrier();
    let candidate = (group.y * groups.x + group.x) * CANDIDATE_THREADS + local;
    let count = min(atomicLoad(&control[COUNT_CANDIDATES]), arrayLength(&candidates));
    if candidate < count {
        process_candidate(pixel_at(candidates[candidate]));
    }
    workgroupBarrier();
    expand_blend_items(local);
}

// ---------------------------------------------------------------------------
// Pass 3: present the image, resolving the queued colors of each touched quad
// (DeferredColorApply2x2CS, done per pixel)

@fragment fn cmaa2_present(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    let color = textureLoad(color_in, pixel, 0);
    let quad = vec2<u32>(pixel) / 2u;
    var header = heads_view[quad.y * quads_wide() + quad.x];
    let slot = (u32(pixel.y) & 1u) * 2u + (u32(pixel.x) & 1u);
    var sum = vec4<f32>(0.0);
    // The bound keeps corrupt data from hanging the GPU, as in the reference.
    for (var i = 0u; header != NO_ITEM && i < MAX_ITEMS_PER_QUAD; i++) {
        let item = items_view[header & 0x1FFFFFFFu];
        if (header >> 30u) == slot {
            let weight = 0.8 + f32((header >> 29u) & 1u);
            sum += vec4<f32>(unpack_color(item.y) * weight, weight);
        }
        header = item.x;
    }
    if sum.a == 0.0 {
        return color;
    }
    return vec4<f32>(sum.rgb / sum.a, color.a);
}
