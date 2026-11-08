struct PageTable {
    counts: vec4<u32>,
    shape: vec4<u32>,
    bounds: vec4<f32>,
    records: array<vec4<u32>>,
}
@group(1) @binding(6) var material_pages: texture_2d_array<u32>;
@group(1) @binding(7) var<storage, read> page_table: PageTable;

fn page_bilinear(uv: vec2<f32>, layer: i32, mip: u32) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(material_pages, i32(mip)));
    let position = uv * vec2<f32>(size) - vec2<f32>(0.5);
    let lower = vec2<i32>(floor(position));
    let weight = fract(position);
    let a = vec4<f32>(textureLoad(material_pages, clamp(lower, vec2<i32>(0), size - 1), layer, i32(mip)));
    let b = vec4<f32>(textureLoad(material_pages, clamp(lower + vec2<i32>(1, 0), vec2<i32>(0), size - 1), layer, i32(mip)));
    let c = vec4<f32>(textureLoad(material_pages, clamp(lower + vec2<i32>(0, 1), vec2<i32>(0), size - 1), layer, i32(mip)));
    let d = vec4<f32>(textureLoad(material_pages, clamp(lower + vec2<i32>(1), vec2<i32>(0), size - 1), layer, i32(mip)));
    return mix(mix(a, b, weight.x), mix(c, d, weight.x), weight.y) / 255.0;
}

fn page_filtered(uv: vec2<f32>, layer: i32, lod: f32) -> vec4<f32> {
    let low = u32(floor(lod));
    let high = min(low + 1u, page_table.shape.w);
    return mix(page_bilinear(uv, layer, low), page_bilinear(uv, layer, high), fract(lod));
}

struct PageLookup {
    chosen: vec4<u32>,
    lod: f32,
    work: u32,
}

fn lookup_page(global: vec2<f32>, footprint: f32) -> PageLookup {
    var result = PageLookup(vec4<u32>(0u), 0.0, 0u);
    var node = 0u;
    for (var level = 0u; level <= 16u; level++) {
        let record = page_table.records[node];
        result.work += 1u;
        let requested_lod = max(0.0, log2(max(footprint * exp2(f32(level)), 1.0)));
        if record.y != 0u && (level == 0u || requested_lod <= f32(page_table.shape.w)) {
            result.chosen = vec4<u32>(page_coordinate(global, level), level, record.y - 1u);
            result.lod = min(requested_lod, f32(page_table.shape.w));
        }
        if record.x == 0u || level == 16u { break; }
        let child = page_coordinate(global, level + 1u) % vec2<u32>(2u);
        node = record.x + child.x + 2u * child.y;
    }
    return result;
}

fn cached_surface(world: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> BlendedSurface {
    if page_table.counts.w != 0u { return cached_anisotropic(world, dx, dy); }
    let global = clamp((world - page_table.bounds.xy) / page_table.bounds.zw, vec2<f32>(0.0), vec2<f32>(1.0));
    let interior = vec2<f32>(page_table.shape.xy);
    let footprint = max(length(dx / page_table.bounds.zw * interior), length(dy / page_table.bounds.zw * interior));
    return cached_at(global, footprint);
}

fn cached_at(global: vec2<f32>, footprint: f32) -> BlendedSurface {
    let interior = vec2<f32>(page_table.shape.xy);
    let result = lookup_page(global, footprint);
    let chosen = result.chosen;
    let lod = result.lod;
    let local = global * exp2(f32(chosen.z)) - vec2<f32>(chosen.xy);
    let border = f32(page_table.shape.z);
    let uv = (local * interior + vec2<f32>(border)) / (interior + vec2<f32>(2.0 * border));
    let color = page_filtered(uv, i32(chosen.w * (2u + page_table.counts.z)), lod);
    let normal = page_filtered(uv, i32(chosen.w * (2u + page_table.counts.z) + 1u), lod);
    return BlendedSurface(color.rgb, normalize(normal.xyz * 2.0 - 1.0), normal.w, select(0.0, color.a, page_table.counts.z != 0u));
}

fn page_coordinate(global: vec2<f32>, depth: u32) -> vec2<u32> {
    let cells = 1u << depth;
    return min(vec2<u32>(floor(global * f32(cells))), vec2<u32>(cells - 1u));
}

fn cached_anisotropic(world: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> BlendedSurface {
    let size = vec2<f32>(page_table.shape.xy);
    let global = (world - page_table.bounds.xy) / page_table.bounds.zw;
    let center = clamp(global, vec2<f32>(0.0), vec2<f32>(1.0));
    let scale = exp2(f32(lookup_page(center, 0.0).chosen.z));
    // Convert texels per world unit, not extreme derivatives divided by small bounds.
    let axes = layered_principal_axes(dx, dy, size * scale / page_table.bounds.zw);
    let footprint = layered_axis_footprint(axes, size * scale, min(f32(page_table.counts.w + 1u), 16.0));
    let radius = max(axes.z / footprint.z, axes.w) / scale;
    let count = u32(footprint.z);
    if count == 1u { return cached_at(center, radius); }
    var color = vec3<f32>(0.0);
    var normal = vec3<f32>(0.0);
    var first_normal = vec3<f32>(0.0);
    var roughness = 0.0;
    var metalness = 0.0;
    for (var tap = 0u; tap < count; tap++) {
        let offset = (f32(tap) + 0.5) / f32(count) - 0.5;
        // Every tap chooses its own resident page and safe mip, including across boundaries.
        let position = clamp(global + footprint.xy * offset, vec2<f32>(0.0), vec2<f32>(1.0));
        let value = cached_at(position, radius);
        color += value.albedo;
        if tap == 0u { first_normal = value.normal; }
        normal += value.normal;
        roughness += value.roughness;
        metalness += value.metalness;
    }
    let direction = cached_normal_direction(normal, first_normal);
    return BlendedSurface(color / f32(count), direction, roughness / f32(count), metalness / f32(count));
}

fn cached_normal_direction(sum: vec3<f32>, first: vec3<f32>) -> vec3<f32> {
    // Exact antipodal ties preserve the first tap, as in mip reduction.
    if dot(sum, sum) > 0.0 { return normalize(sum); }
    return first;
}
