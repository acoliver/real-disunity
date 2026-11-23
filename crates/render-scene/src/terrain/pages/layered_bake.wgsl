
struct LayeredPositions {
    footprint: vec2<f32>,
    points: array<vec2<f32>>,
}
@group(0) @binding(3) var<storage, read> layered_positions: LayeredPositions;
@group(0) @binding(4) var<storage, read_write> layered_products: array<vec4<u32>>;

// A layer's coverage reads only grid cells within its reach of the texel, and is exactly zero
// when none of them holds the layer's index. A workgroup therefore marks the indices held by the
// cells within reach of any of its texels (grid indices are bytes, so 256 bits hold them all)
// and composes only those layers, in the same ascending order with the same arithmetic.
var<workgroup> layered_cells: array<atomic<i32>, 4>;
var<workgroup> layered_widest: atomic<u32>;
var<workgroup> layered_present: array<atomic<u32>, 8>;

fn layered_next(index: u32) -> u32 {
    var word = index / 32u;
    var bits = 0u;
    if word < 8u { bits = atomicLoad(&layered_present[word]) & (0xffffffffu << (index % 32u)); }
    while bits == 0u && word < 7u {
        word += 1u;
        bits = atomicLoad(&layered_present[word]);
    }
    return select(0xffffffffu, word * 32u + countTrailingZeros(bits), bits != 0u);
}
/// The base cell of a texel's bilinear membership, as `layered_membership` floors it.
fn layered_base_cell(world: vec2<f32>) -> vec2<i32> {
    let grid = (world - layered_source.mapping.xy) / layered_source.mapping.z;
    let p = clamp(grid - 0.5, vec2<f32>(-1.0), vec2<f32>(layered_source.grid.xy));
    return clamp(vec2<i32>(floor(p)), vec2<i32>(0), vec2<i32>(layered_source.grid.xy) - 1);
}
/// Cells from a base cell to the farthest cell a layer of blur `radius` can read. Spiral taps lie
/// within 0.93 of the jittered radius, at most 1.125 times the authored one; the bilinear read
/// adds one cell and three cells plus four ulps of the farthest coordinate cover rounding.
fn layered_reach(radius: f32) -> i32 {
    let size = f32(max(layered_source.grid.x, layered_source.grid.y));
    let taps = select(0.0, radius / layered_source.mapping.z * 1.125, layered_source.grid.w != 0u);
    return i32(min(ceil(taps + (size + taps) * 4.8e-7), size)) + 3;
}
fn layered_mark_present(lane: u32) {
    let low = vec2<i32>(atomicLoad(&layered_cells[0]), atomicLoad(&layered_cells[1]));
    let high = vec2<i32>(atomicLoad(&layered_cells[2]), atomicLoad(&layered_cells[3]));
    let widest = layered_reach(bitcast<f32>(atomicLoad(&layered_widest)));
    let first = max(low - widest, vec2<i32>(0));
    let last = min(high + widest, vec2<i32>(layered_source.grid.xy) - 1);
    let width = u32(last.x - first.x + 1);
    let cells = width * u32(last.y - first.y + 1);
    var marked = 0xffffffffu;
    for (var i = lane; i < cells; i += 64u) {
        let at = first + vec2<i32>(vec2<u32>(i % width, i / width));
        let value = layered_grid[layered_source.grid.z + u32(at.y) * layered_source.grid.x + u32(at.x)];
        if value == marked { continue; }
        let reach = layered_reach(layered_source.layers[value].tint_radius.w);
        if all(at >= low - reach) && all(at <= high + reach) {
            atomicOr(&layered_present[value / 32u], 1u << (value % 32u));
            marked = value;
        }
    }
}

@compute @workgroup_size(64)
fn bake_layered(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let valid = id.x < arrayLength(&layered_positions.points);
    if lane == 0u {
        atomicStore(&layered_cells[0], 0x7fffffff);
        atomicStore(&layered_cells[1], 0x7fffffff);
        atomicStore(&layered_cells[2], -1);
        atomicStore(&layered_cells[3], -1);
        atomicStore(&layered_widest, 0u);
    }
    if lane < 8u { atomicStore(&layered_present[lane], 0u); }
    workgroupBarrier();
    // Blur radii are nonnegative, so their bits order as the radii do.
    for (var layer = lane; layer < arrayLength(&layered_source.layers); layer += 64u) {
        atomicMax(&layered_widest, bitcast<u32>(layered_source.layers[layer].tint_radius.w));
    }
    if valid {
        let cell = layered_base_cell(layered_positions.points[id.x]);
        atomicMin(&layered_cells[0], cell.x);
        atomicMin(&layered_cells[1], cell.y);
        atomicMax(&layered_cells[2], cell.x);
        atomicMax(&layered_cells[3], cell.y);
    }
    workgroupBarrier();
    layered_mark_present(lane);
    workgroupBarrier();
    if !valid { return; }
    let result = compose_layered(layered_positions.points[id.x],
        vec2<f32>(layered_positions.footprint.x, 0.0), vec2<f32>(0.0, layered_positions.footprint.y));
    layered_products[id.x] = vec4<u32>(pack4x8unorm(vec4<f32>(result.albedo, result.metallic)),
        pack4x8unorm(vec4<f32>(result.normal * 0.5 + 0.5, result.roughness)),
        u32(round(clamp(result.height, 0.0, 1.0) * 255.0)), 0u);
}
