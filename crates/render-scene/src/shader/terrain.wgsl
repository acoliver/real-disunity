// Generic terrain coverage filtering and normalized metallic-roughness blending.
struct TerrainSurface {
    // rgb is the authored tint as a fraction (neutral is 1.0, not 0.0), and w
    // is the blur radius already converted from world distance into grid
    // texels, because this shader walks the grid in texels.
    tint_and_blur: vec4<f32>,
    // Authored cosine, sine and palette layer. Surface zero also stores
    // terrain-wide strength bits XOR 1.0 bits, preserving neutral-zero padding.
    rotation_and_layer: vec3<f32>,
    normal_strength_bits: u32,
}

struct Terrain {
    // Grid origin east and north in metres, the sample spacing in metres, and
    // the material UV scale in tiles per metre.
    origin_and_spacing: vec4<f32>,
    // Grid columns and rows, how many of the surface slots are addressed, and
    // one word of padding.
    dimensions_and_count: vec4<u32>,
    surfaces: array<TerrainSurface, 256>,
}

// The terrain binds at the same group the environment binds its material to,
// because terrain is the alternative to a material rather than an addition to
// one. Nothing draws with both.
@group(1) @binding(0) var<uniform> terrain: Terrain;

// Unfilterable by design. This texture holds identifiers, not quantities:
// filtering between surfaces 7 and 84 would produce 45, naming an unrelated
// material that may not even be bound. The blending belongs between the masks
// those identifiers select, which is what the rest of this file does.
@group(1) @binding(1) var type_grid: texture_2d<u32>;

@group(1) @binding(2) var albedo_array: texture_2d_array<f32>;
@group(1) @binding(3) var normal_array: texture_2d_array<f32>;
@group(1) @binding(4) var rough_metal_array: texture_2d_array<f32>;
@group(1) @binding(5) var terrain_sampler: sampler;

// Converts a scene-space position to fractional grid coordinates. This is the
// exact inverse of the placement the mesh builder uses, and it has to be: a
// disagreement here would slide the materials off the landform they belong to,
// which looks like bad authoring rather than like a bug in this line.
fn world_to_grid(world_east: f32, world_north: f32) -> vec2<f32> {
    let origin = terrain.origin_and_spacing;
    return vec2<f32>(
        (world_east - origin.x) / origin.z,
        (world_north - origin.y) / origin.z,
    );
}

// Reads the surface index at integer grid coordinates, clamping to the edge.
// Out-of-range reads take the border surface rather than a default, so the
// blend at the world's edge is the edge material continuing outward instead of
// an abrupt seam against whatever surface zero happens to be.
fn read_surface_at(column: i32, row: i32) -> u32 {
    let dimensions = terrain.dimensions_and_count;
    let clamped_column = clamp(column, 0, i32(dimensions.x) - 1);
    let clamped_row = clamp(row, 0, i32(dimensions.y) - 1);
    return textureLoad(type_grid, vec2<i32>(clamped_column, clamped_row), 0).r;
}

// Bilinear equality mask for one surface: the fraction of the four texels
// bracketing this point that name it, weighted by proximity.
//
// The half-texel shift makes the four integer reads the texels whose centres
// bracket the requested coordinate, which is what a hardware bilinear fetch
// would have sampled. Without it the mask is offset by half a cell and every
// boundary sits half a texel from where the grid puts it.
fn surface_mask_point(surface: u32, grid: vec2<f32>) -> f32 {
    let g = grid - vec2<f32>(0.5, 0.5);
    let column = i32(floor(g.x));
    let row = i32(floor(g.y));
    let fraction = g - vec2<f32>(f32(column), f32(row));

    let m00 = select(0.0, 1.0, read_surface_at(column, row) == surface);
    let m10 = select(0.0, 1.0, read_surface_at(column + 1, row) == surface);
    let m01 = select(0.0, 1.0, read_surface_at(column, row + 1) == surface);
    let m11 = select(0.0, 1.0, read_surface_at(column + 1, row + 1) == surface);

    return mix(
        mix(m00, m10, fraction.x),
        mix(m01, m11, fraction.x),
        fraction.y,
    );
}

// A separable 3x3 tent filter: weights [1, 2, 1] on each axis.
// Each surface owns the sampling radius in grid texels. Zero bypasses blur.
fn surface_mask(surface: u32, grid: vec2<f32>) -> f32 {
    let radius = terrain.surfaces[surface].tint_and_blur.w;
    if radius <= 0.0 {
        return surface_mask_point(surface, grid);
    }
    var weighted = 0.0;
    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let weight = f32((2 - abs(x)) * (2 - abs(y)));
            let offset = vec2<f32>(f32(x), f32(y)) * radius;
            weighted += surface_mask_point(surface, grid + offset) * weight;
        }
    }
    return weighted / 16.0;
}

fn rotate(v: vec2<f32>, cosine: f32, sine: f32) -> vec2<f32> {
    return vec2<f32>(
        v.x * cosine - v.y * sine,
        v.x * sine + v.y * cosine,
    );
}

struct SurfaceSample {
    albedo: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metalness: f32,
}

// Samples one surface's palette layer at this fragment.
//
// The UV is the world position rotated about the world origin - not about the
// centre of a tile - and then scaled into tiles. Rotating about a tile centre
// would be the intuitive reading and is wrong: it would make the rotation a
// per-tile shuffle instead of a single rotation of the whole material field,
// and adjacent tiles would no longer line up.
//
// The gradients arrive from the caller already computed. They cannot be taken
// here: this runs inside a loop whose trip count depends on which surfaces meet
// at the fragment, so it is non-uniform control flow, where derivative
// operations are undefined. Rotating the caller's uniform gradients per surface
// reproduces what an implicit-derivative sample would have selected without ever
// asking for a derivative in a place that cannot supply one.
fn sample_surface(
    surface: u32,
    world_xy: vec2<f32>,
    world_xy_dx: vec2<f32>,
    world_xy_dy: vec2<f32>,
) -> SurfaceSample {
    let record = terrain.surfaces[surface];
    let cosine = record.rotation_and_layer.x;
    let sine = record.rotation_and_layer.y;
    let layer = i32(record.rotation_and_layer.z);
    let uv_scale = terrain.origin_and_spacing.w;

    let uv = rotate(world_xy, cosine, sine) * uv_scale;
    let gradient_scale = exp2(frame.lighting_options.w);
    let uv_dx = rotate(world_xy_dx, cosine, sine) * uv_scale * gradient_scale;
    let uv_dy = rotate(world_xy_dy, cosine, sine) * uv_scale * gradient_scale;

    let albedo_sample = textureSampleGrad(albedo_array, terrain_sampler, uv, layer, uv_dx, uv_dy);
    let normal_sample = textureSampleGrad(normal_array, terrain_sampler, uv, layer, uv_dx, uv_dy);
    let rough_metal = textureSampleGrad(rough_metal_array, terrain_sampler, uv, layer, uv_dx, uv_dy);

    // BC5 stores two channels; sampling .b would return zero and lay every
    // normal flat into the tangent plane, so Z is reconstructed from the
    // unit-length constraint exactly as the environment shader does.
    let normal_xy = normal_sample.rg * 2.0 - 1.0;
    let normal_z = sqrt(max(0.0, 1.0 - dot(normal_xy, normal_xy)));

    var sample: SurfaceSample;
    sample.albedo = albedo_sample.rgb * record.tint_and_blur.rgb;
    sample.normal = vec3<f32>(rotate(normal_xy, cosine, -sine), normal_z);
    sample.roughness = rough_metal.g;
    sample.metalness = rough_metal.b;
    return sample;
}

struct BlendAccumulator {
    albedo: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metalness: f32,
    strength_so_far: f32,
    any_contributed: bool,
}

fn blend_surface(
    accumulator: ptr<function, BlendAccumulator>,
    surface: u32,
    grid: vec2<f32>,
    world_xy: vec2<f32>,
    world_xy_dx: vec2<f32>,
    world_xy_dy: vec2<f32>,
) {
    let strength = surface_mask(surface, grid);
    if strength <= 0.0 {
        return;
    }
    let sample = sample_surface(surface, world_xy, world_xy_dx, world_xy_dy);
    (*accumulator).albedo += sample.albedo * strength;
    (*accumulator).normal += sample.normal * strength;
    (*accumulator).roughness += sample.roughness * strength;
    (*accumulator).metalness += sample.metalness * strength;
    (*accumulator).strength_so_far += strength;
    (*accumulator).any_contributed = true;
}

struct BlendedSurface {
    albedo: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metalness: f32,
}

// Normalize the accumulated surface weights once, independent of palette order.
fn blend_surfaces(
    grid: vec2<f32>,
    world_xy: vec2<f32>,
    world_xy_dx: vec2<f32>,
    world_xy_dy: vec2<f32>,
    centre_surface: u32,
) -> BlendedSurface {
    var accumulator: BlendAccumulator;
    accumulator.albedo = vec3<f32>(0.0);
    accumulator.normal = vec3<f32>(0.0);
    accumulator.roughness = 0.0;
    accumulator.metalness = 0.0;
    accumulator.strength_so_far = 0.0;
    accumulator.any_contributed = false;

    let count = terrain.dimensions_and_count.z;
    for (var surface: u32 = 0u; surface < count; surface = surface + 1u) {
        blend_surface(
            &accumulator,
            surface,
            grid,
            world_xy,
            world_xy_dx,
            world_xy_dy,
        );
    }

    var result: BlendedSurface;
    if accumulator.any_contributed {
        result.albedo = accumulator.albedo / accumulator.strength_so_far;
        result.normal = normalize(accumulator.normal);
        result.roughness = accumulator.roughness / accumulator.strength_so_far;
        result.metalness = accumulator.metalness / accumulator.strength_so_far;
        return result;
    }

    // Different per-surface radii can leave a coverage gap; retain the local material.
    let sample = sample_surface(centre_surface, world_xy, world_xy_dx, world_xy_dy);
    result.albedo = sample.albedo;
    result.normal = sample.normal;
    result.roughness = sample.roughness;
    result.metalness = sample.metalness;
    return result;
}

struct TerrainVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
}

struct TerrainInstanceInput {
    @location(3) model_column_0: vec4<f32>,
    @location(4) model_column_1: vec4<f32>,
    @location(5) model_column_2: vec4<f32>,
    @location(6) model_column_3: vec4<f32>,
    @location(10) plane_index: vec4<f32>,
}

struct TerrainVertexOutput {
    @invariant @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) @interpolate(flat) plane_index: u32,
    @location(3) skirt_axis: vec2<f32>,
}

// Terrain shares the environment's vertex and instance layout so that one
// buffer packing serves both paths. The instance is the identity placement: the
// heightfield is authored in scene space already, and giving it a transform
// would introduce a second place where terrain could be positioned.
@vertex
fn vs_terrain(
    vertex: TerrainVertexInput,
    instance: TerrainInstanceInput,
    @builtin(vertex_index) vertex_index: u32,
) -> TerrainVertexOutput {
    let model = mat4x4<f32>(
        instance.model_column_0,
        instance.model_column_1,
        instance.model_column_2,
        instance.model_column_3,
    );
    let world_position = (model * vec4<f32>(vertex.position, 1.0)).xyz;

    var out: TerrainVertexOutput;
    out.clip_position = frame.view_projection * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.plane_index = u32(instance.plane_index.x) | (u32(instance.plane_index.y) << 16u);
    let descriptor = terrain_plane_data[3] + out.plane_index * 8u;
    let width = terrain_plane_data[descriptor + 5u];
    let height = terrain_plane_data[descriptor + 6u];
    out.skirt_axis = vec2<f32>(0.0);
    if vertex_index >= width * height {
        let edge_vertex = vertex_index - width * height;
        let south = 2u * (width - 1u);
        let east = south + 2u * (height - 1u);
        let north = east + south;
        out.skirt_axis = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0),
            (edge_vertex >= south && edge_vertex < east) || edge_vertex >= north);
    }
    out.world_normal = normalize(
        mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz) * vertex.normal,
    );
    return out;
}

// These are the CPU's final f32 coordinates, not a second conversion of the
// integer heightfield. Axis positions include shortened cells and origin rounding.
@group(1) @binding(23) var<storage, read> terrain_plane_data: array<u32>;

fn terrain_sample_position(sample: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(
        bitcast<f32>(terrain_plane_data[terrain_plane_data[0] + sample.x]),
        bitcast<f32>(terrain_plane_data[terrain_plane_data[1] + sample.y]),
        bitcast<f32>(terrain_plane_data[terrain_plane_data[2] + sample.y * terrain.dimensions_and_count.x + sample.x]),
    );
}

// Origin subtraction and division can round across a stored mesh boundary.
// Correct the candidate against selected samples; the outer edge stays in the last cell.
fn terrain_axis_cell(world: f32, axis: u32, candidate: u32, window: vec3<u32>, cells: u32) -> u32 {
    let offset = terrain_plane_data[axis];
    var cell = candidate;
    while cell > 0u && world < bitcast<f32>(terrain_plane_data[offset + window.x + cell * window.z]) {
        cell -= 1u;
    }
    while cell + 1u < cells && world >= bitcast<f32>(terrain_plane_data[offset + min(window.x + (cell + 1u) * window.z, window.y)]) {
        cell += 1u;
    }
    return cell;
}

fn terrain_cell(world_xy: vec2<f32>, descriptor: u32) -> vec2<u32> {
    let minimum = vec2<u32>(terrain_plane_data[descriptor], terrain_plane_data[descriptor + 1u]);
    let maximum = vec2<u32>(terrain_plane_data[descriptor + 2u], terrain_plane_data[descriptor + 3u]);
    let stride = terrain_plane_data[descriptor + 4u];
    let cells = vec2<u32>(terrain_plane_data[descriptor + 5u], terrain_plane_data[descriptor + 6u]) - 1u;
    let grid = max(world_to_grid(world_xy.x, world_xy.y) - vec2<f32>(minimum), vec2<f32>(0.0));
    let candidate = min(vec2<u32>(floor(grid / f32(stride))), cells - 1u);
    return vec2<u32>(
        terrain_axis_cell(world_xy.x, 0u, candidate.x, vec3<u32>(minimum.x, maximum.x, stride), cells.x),
        terrain_axis_cell(world_xy.y, 1u, candidate.y, vec3<u32>(minimum.y, maximum.y, stride), cells.y),
    );
}

fn terrain_face_normal(world_xy: vec2<f32>, plane_index: u32, skirt_axis: vec2<f32>) -> vec3<f32> {
    if skirt_axis.x > 0.0 { return vec3<f32>(1.0, 0.0, 0.0); }
    if skirt_axis.y > 0.0 { return vec3<f32>(0.0, 1.0, 0.0); }
    let descriptor = terrain_plane_data[3] + plane_index * 8u;
    let minimum = vec2<u32>(terrain_plane_data[descriptor], terrain_plane_data[descriptor + 1u]);
    let maximum = vec2<u32>(terrain_plane_data[descriptor + 2u], terrain_plane_data[descriptor + 3u]);
    let stride = terrain_plane_data[descriptor + 4u];
    let cell = terrain_cell(world_xy, descriptor);
    let lo = minimum + cell * stride;
    let hi = min(lo + stride, maximum);
    let a = terrain_sample_position(lo);
    let c = terrain_sample_position(hi);
    let fraction = (world_xy - a.xy) / (c.xy - a.xy);
    if fraction.y <= fraction.x {
        let b = terrain_sample_position(vec2<u32>(hi.x, lo.y));
        return cross(b - a, c - a);
    }
    let d = terrain_sample_position(vec2<u32>(lo.x, hi.y));
    return cross(c - a, d - a);
}

// One full-resolution height, clamped to the grid.
fn terrain_height_at(sample: vec2<i32>) -> f32 {
    let last = vec2<i32>(terrain.dimensions_and_count.xy) - vec2<i32>(1);
    let clamped = vec2<u32>(clamp(sample, vec2<i32>(0), last));
    return bitcast<f32>(terrain_plane_data[terrain_plane_data[2] + clamped.y * terrain.dimensions_and_count.x + clamped.x]);
}

// The heightfield normal at one sample, from central differences that shorten to
// one-sided differences at the grid edge, as the CPU mesh builder computes it.
fn terrain_sample_normal(sample: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(terrain.dimensions_and_count.xy) - vec2<i32>(1);
    let low = max(sample - vec2<i32>(1), vec2<i32>(0));
    let high = min(sample + vec2<i32>(1), last);
    let run = vec2<f32>(max(high - low, vec2<i32>(1))) * terrain.origin_and_spacing.z;
    let east = terrain_height_at(vec2<i32>(high.x, sample.y)) - terrain_height_at(vec2<i32>(low.x, sample.y));
    let north = terrain_height_at(vec2<i32>(sample.x, high.y)) - terrain_height_at(vec2<i32>(sample.x, low.y));
    return vec3<f32>(-east / run.x, -north / run.y, 1.0);
}

// Terrain is lit from the full-resolution heightfield at every pixel, so a chunk drawn
// with coarser triangles keeps the lighting detail of the finest one; the vertex normal
// of a coarse chunk only describes its own facets. Skirts keep their vertical normal.
fn terrain_pixel_normal(world_xy: vec2<f32>, skirt_axis: vec2<f32>, interpolated: vec3<f32>) -> vec3<f32> {
    if any(skirt_axis > vec2<f32>(0.0)) { return normalize(interpolated); }
    let grid = world_to_grid(world_xy.x, world_xy.y);
    let base = vec2<i32>(floor(grid));
    let weight = grid - floor(grid);
    let south = mix(terrain_sample_normal(base), terrain_sample_normal(base + vec2<i32>(1, 0)), weight.x);
    let north = mix(terrain_sample_normal(base + vec2<i32>(0, 1)), terrain_sample_normal(base + vec2<i32>(1, 1)), weight.x);
    return normalize(mix(south, north, weight.y));
}

// Evaluate the same adjacent pixel pair on both lanes of a 2x2 quad. Solving
// for displacement avoids subtracting two kilometre-scale world positions.
fn terrain_quad_footprint(fragment: vec4<f32>, axis: vec2<f32>, normal: vec3<f32>) -> vec2<f32> {
    let size = frame.raster_viewport.xy;
    let lane = dot(vec2<f32>(vec2<u32>(fragment.xy) & vec2<u32>(1u)), axis);
    let direction = 1.0 - 2.0 * lane;
    let delta = axis * vec2<f32>(2.0 / size.x, -2.0 / size.y) * direction;
    let ndc = vec2<f32>(fragment.x / size.x * 2.0 - 1.0, 1.0 - fragment.y / size.y * 2.0) + delta;
    let rows = transpose(frame.view_projection);
    let x = rows[0].xyz - ndc.x * rows[3].xyz;
    let y = rows[1].xyz - ndc.y * rows[3].xyz;
    let tangent_x = cross(y, normal);
    let tangent_y = cross(normal, x);
    let determinant = dot(x, tangent_x);
    let floor = length(cross(x, y)) * length(normal) * 1e-5;
    let safe = select(-1.0, 1.0, determinant >= 0.0) * max(abs(determinant), floor);
    let displacement = (delta.x * tangent_x + delta.y * tangent_y) / (fragment.w * safe);
    return clamp(displacement.xy * direction, vec2<f32>(-1e4), vec2<f32>(1e4));
}

@fragment
fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {
    let world_xy = input.world_position.xy;
    let footprint_normal = normalize(terrain_face_normal(input.world_position.xy, input.plane_index, input.skirt_axis));
    let world_xy_dx = terrain_quad_footprint(input.clip_position, vec2<f32>(1.0, 0.0), footprint_normal);
    let world_xy_dy = terrain_quad_footprint(input.clip_position, vec2<f32>(0.0, 1.0), footprint_normal);

    let grid = world_to_grid(world_xy.x, world_xy.y);
    let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));

    let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);

    // Layer normals have already been unrotated into the shared east/north
    // frame before blending or page storage. Follow east along the heightfield;
    // the right-handed bitangent then follows north on flat ground.
    let geometric = terrain_pixel_normal(world_xy, input.skirt_axis, input.world_normal);
    let tangent = normalize(vec3<f32>(geometric.z, 0.0, -geometric.x));
    let bitangent = cross(geometric, tangent);
    let strength = bitcast<f32>(terrain.surfaces[0].normal_strength_bits ^ 0x3f800000u);
    var normal = geometric;
    if strength != 0.0 {
        normal = normalize(
            (tangent * blended.normal.x + bitangent * blended.normal.y) * strength
                + geometric * blended.normal.z,
        );
    }

    // Terrain has neither an ambient occlusion map nor a screen-space
    // occlusion pass, so its ambient contribution remains unoccluded.
    let projected = scene_project_decals(input.world_position, geometric, 2u,
        blended.albedo, vec2<f32>(blended.roughness, blended.metalness), normal);
    let lighting = scene_surface_lighting(
        input.world_position,
        projected.normal,
        projected.albedo,
        vec3<f32>(projected.properties, 1.0), DIELECTRIC_F0,
    );

    let colour = lighting.color;
    // Terrain has no emissive term, so its total outgoing radiance is exactly
    // what surface_lighting returned; exposure still goes through the shared
    // function so terrain and the props standing on it cannot drift apart.
    return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);
}
