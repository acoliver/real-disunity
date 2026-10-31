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
}

struct TerrainVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
}

// Terrain shares the environment's vertex and instance layout so that one
// buffer packing serves both paths. The instance is the identity placement: the
// heightfield is authored in scene space already, and giving it a transform
// would introduce a second place where terrain could be positioned.
@vertex
fn vs_terrain(
    vertex: TerrainVertexInput,
    instance: TerrainInstanceInput,
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
    out.world_normal = normalize(
        mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz) * vertex.normal,
    );
    return out;
}

@fragment
fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {
    let world_xy = input.world_position.xy;

    // Both derivatives are taken here, at the top, in control flow every
    // fragment in the quad reaches alike. Everything below this point is
    // conditioned on which surfaces are present, and a derivative taken there
    // would be reading neighbouring lanes that may have taken a different
    // branch - undefined, and in practice a mip selection that flickers along
    // exactly the boundaries this shader exists to smooth.
    let world_xy_dx = dpdx(world_xy);
    let world_xy_dy = dpdy(world_xy);

    let grid = world_to_grid(world_xy.x, world_xy.y);
    let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));

    let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);

    // Layer normals have already been unrotated into the shared east/north
    // frame before blending or page storage. Follow east along the heightfield;
    // the right-handed bitangent then follows north on flat ground.
    let geometric = normalize(input.world_normal);
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
