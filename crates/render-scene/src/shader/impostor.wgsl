override SCENE_IMPOSTOR: bool = true;
const SCENE_ATLAS_MIP_FLAG: u32 = 4u;
struct AtlasFrames {
    cells: array<vec2<f32>, 4>,
    weights: vec4<f32>,
}

fn scene_atlas_frames(view: vec3<f32>, grid: vec2<f32>, octahedral: bool, blend: bool, snapping: f32) -> AtlasFrames {
    var coordinate: vec2<f32>;
    if octahedral {
        let v = vec3<f32>(view.xy, max(view.z, 0.0001));
        let p = v.xy / dot(abs(v), vec3<f32>(1.0));
        coordinate = clamp((vec2<f32>(p.x + p.y, p.x - p.y) + 1.0) * 0.5, vec2<f32>(0.0), vec2<f32>(1.0)) * (grid - 1.0);
    } else {
        let direction = normalize(view);
        let azimuth = fract(atan2(direction.y, direction.x) / (2.0 * PI) + 1.0);
        let elevation = asin(clamp(direction.z, 0.0, 1.0)) / (0.5 * PI);
        coordinate = vec2<f32>(azimuth * grid.x, elevation * (grid.y - 1.0));
    }
    let nearest = round(coordinate);
    coordinate = mix(coordinate, nearest, snapping);
    var result: AtlasFrames;
    if !blend {
        result.cells[0] = nearest;
        result.weights = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    } else {
        let base = floor(coordinate);
        let f = fract(coordinate);
        result.cells[0] = base;
        if octahedral {
            result.cells[1] = base + select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), f.x > f.y);
            result.cells[2] = base + 1.0;
            result.weights = vec4<f32>(min(1.0 - f.x, 1.0 - f.y), abs(f.x - f.y), min(f.x, f.y), 0.0);
        } else {
            result.cells[1] = base + vec2<f32>(1.0, 0.0);
            result.cells[2] = base + vec2<f32>(0.0, 1.0);
            result.cells[3] = base + 1.0;
            result.weights = vec4<f32>((1.0-f.x)*(1.0-f.y), f.x*(1.0-f.y), (1.0-f.x)*f.y, f.x*f.y);
        }
    }
    for (var i = 0u; i < 4u; i++) {
        if octahedral { result.cells[i] = min(result.cells[i], grid - 1.0); }
        else { result.cells[i] = vec2<f32>(result.cells[i].x % grid.x, min(result.cells[i].y, grid.y - 1.0)); }
    }
    return result;
}

fn scene_atlas_uv(uv: vec2<f32>, cell: vec2<f32>, grid: vec2<f32>, inset: vec2<f32>) -> vec2<f32> {
    return (cell + clamp(uv, inset, 1.0 - inset)) / grid;
}

fn scene_atlas_plane_uv(cell: vec2<f32>, grid: vec2<f32>, eye: vec3<f32>, point: vec3<f32>, size: vec2<f32>) -> vec2<f32> {
    let p = (cell / (grid - 1.0) - 0.5) * 2.0;
    let q = vec2<f32>(p.x + p.y, p.x - p.y) * 0.5;
    let normal = normalize(vec3<f32>(q, 1.0 - dot(abs(q), vec2<f32>(1.0))));
    var right = vec3<f32>(-normal.y, normal.x, 0.0);
    if dot(right, right) == 0.0 { right = vec3<f32>(1.0, 0.0, 0.0); }
    right = normalize(right);
    let up = normalize(cross(right, normal));
    let ray = point - eye;
    let hit = eye - ray * (dot(normal, eye) / dot(normal, ray));
    let uv = vec2<f32>(dot(right, hit), dot(up, hit)) / size + 0.5;
    return vec2<f32>(1.0 - uv.x, uv.y);
}

// Capture all clamped footprints before any fragment-dependent control flow.
// Material-uniform gates can skip footprints; explicit samples can branch per fragment.
struct AtlasFootprint {
    uv: array<vec2<f32>, 4>,
    dx: array<vec2<f32>, 4>,
    dy: array<vec2<f32>, 4>,
    stock_dx: vec2<f32>,
    stock_dy: vec2<f32>,
}
fn scene_atlas_coordinates(map: texture_2d<f32>, input: VertexOutput) -> array<vec2<f32>, 4> {
    let selected = scene_atlas_selection(input);
    let inset = scene_atlas_inset(vec2<f32>(textureDimensions(map)));
    let planes = array<vec2<f32>, 4>(input.atlas_uv01.xy, input.atlas_uv01.zw, input.atlas_uv23.xy, input.atlas_uv23.zw);
    var coordinates: array<vec2<f32>, 4>;
    for (var i = 0u; i < 4u; i++) {
        let uv = select(input.texcoord, planes[i], material.impostor_grid.w == 2.0);
        coordinates[i] = scene_atlas_uv(uv, selected.cells[i], material.impostor_grid.xy, inset);
    }
    return coordinates;
}
fn scene_atlas_footprint(map: texture_2d<f32>, input: VertexOutput) -> AtlasFootprint {
    let bias = exp2(frame.lighting_options.w);
    var footprint: AtlasFootprint;
    footprint.stock_dx = dpdx(input.texcoord) * bias;
    footprint.stock_dy = dpdy(input.texcoord) * bias;
    if !SCENE_IMPOSTOR { return footprint; }
    if material.impostor_grid.w != 0.0 {
        let uv = scene_atlas_coordinates(map, input);
        footprint.uv = uv;
        if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u {
            footprint.dx = array<vec2<f32>, 4>(dpdx(uv[0]) * bias, dpdx(uv[1]) * bias, dpdx(uv[2]) * bias, dpdx(uv[3]) * bias);
            footprint.dy = array<vec2<f32>, 4>(dpdy(uv[0]) * bias, dpdy(uv[1]) * bias, dpdy(uv[2]) * bias, dpdy(uv[3]) * bias);
        }
    }
    return footprint;
}
fn scene_atlas_sample(map: texture_2d<f32>, input: VertexOutput, footprint: AtlasFootprint) -> vec4<f32> {
    if !SCENE_IMPOSTOR { return textureSampleGrad(map, material_sampler, input.texcoord, footprint.stock_dx, footprint.stock_dy); }
    if material.impostor_grid.w == 0.0 {
        return textureSampleGrad(map, material_sampler, input.texcoord, footprint.stock_dx, footprint.stock_dy);
    }
    if input.atlas_eye.w == 0.0 {
        return textureSampleGrad(map, material_sampler, input.texcoord, footprint.stock_dx, footprint.stock_dy);
    }
    let selected = scene_atlas_selection(input);
    var value = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        if selected.weights[i] > 0.0 {
            var sampled = textureSampleLevel(map, material_sampler, footprint.uv[i], 0.0);
            if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u {
                sampled = textureSampleGrad(map, material_sampler, footprint.uv[i], footprint.dx[i], footprint.dy[i]);
            }
            value += sampled * selected.weights[i];
        }
    }
    return value;
}


fn scene_atlas_selection(input: VertexOutput) -> AtlasFrames {
    let grid = material.impostor_grid.xy;
    let octahedral = material.impostor_grid.w == 2.0;
    var snapping = 0.0;
    if material.impostor_settings.y > 0.0 {
        let t = clamp(length(input.atlas_eye.xyz) * input.atlas_eye.w / material.impostor_settings.y, 0.0, 1.0) - 0.5;
        snapping = t * t;
    }
    return scene_atlas_frames(input.atlas_eye.xyz * input.atlas_eye.w, grid, octahedral, material.impostor_settings.x != 0.0, snapping);
}

struct AtlasCoordinates { first: vec4<f32>, second: vec4<f32>, }
fn scene_atlas_vertex_uv(input: VertexOutput) -> AtlasCoordinates {
    var coordinates = array<vec2<f32>, 4>(input.texcoord, input.texcoord, input.texcoord, input.texcoord);
    if material.impostor_grid.w == 2.0 {
        let selected = scene_atlas_selection(input);
        for (var i = 0u; i < 4u; i++) {
            if selected.weights[i] > 0.0 {
                coordinates[i] = scene_atlas_plane_uv(selected.cells[i], material.impostor_grid.xy, input.atlas_eye.xyz, input.atlas_point, material.impostor_settings.zw);
            }
        }
    }
    return AtlasCoordinates(vec4<f32>(coordinates[0], coordinates[1]), vec4<f32>(coordinates[2], coordinates[3]));
}

// Equal-area sphere decode. Decode each capture before blending normals.
fn scene_relief_decode(encoded: vec2<f32>) -> vec3<f32> {
    let ab = encoded * 2.828427 - 1.41421356;
    let z = 1.0 - dot(ab, ab);
    return vec3<f32>(ab * sqrt(max(0.0, 1.0 + z)), z);
}
fn scene_relief_normal(input: VertexOutput, footprint: AtlasFootprint) -> vec3<f32> {
    let selected = scene_atlas_selection(input);
    var normal = vec3<f32>(0.0);
    for (var i=0u; i<4u; i++) {
        if selected.weights[i] > 0.0 {
            var sampled = textureSampleLevel(normal_texture, material_sampler, footprint.uv[i], 0.0);
            if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u {
                sampled = textureSampleGrad(normal_texture, material_sampler, footprint.uv[i], footprint.dx[i], footprint.dy[i]);
            }
            normal += scene_relief_decode(sampled.rg) * selected.weights[i];
        }
    }
    let n = normalize(normal);
    let capture = mat3x3<f32>(input.capture_rotation_0, input.capture_rotation_1, cross(input.capture_rotation_0, input.capture_rotation_1));
    return normalize(transpose(capture) * n);
}
fn scene_relief_position(input: VertexOutput) -> vec3<f32> {
    if !SCENE_IMPOSTOR { return input.world_position; }
    if material.impostor_grid.w == 0.0 { return input.world_position; }
    if material.impostor_relief.x == 0.0 { return input.world_position; }
    let footprint = scene_atlas_footprint(impostor_depth, input);
    if input.atlas_eye.w == 0.0 { return input.world_position; }
    let depth = scene_atlas_sample(impostor_depth, input, footprint).r;
    return scene_relief_offset(input.world_position, depth * material.impostor_relief.x);
}
struct SceneReliefDepth { value: f32, visible: bool, }
fn scene_relief_depth(input: VertexOutput) -> SceneReliefDepth {
    let position = scene_relief_position(input);
    if input.atlas_eye.w == 0.0 || material.impostor_relief.x == 0.0 { return SceneReliefDepth(input.clip_position.z, true); }
    let clip = frame.view_projection * vec4<f32>(position,1.0);
    return SceneReliefDepth(clip.z / clip.w, clip.w > 0.0);
}
struct SceneSurfaceOutput { @location(0) colour: vec4<f32>, @builtin(frag_depth) depth: f32, }

fn scene_relief_offset(position: vec3<f32>, amount: f32) -> vec3<f32> {
    let rows = transpose(frame.view_projection);
    if scene_orthographic() {
        return position - normalize(rows[2].xyz) * amount;
    }
    let ray = position - frame.camera_position.xyz;
    return position - ray * (amount / dot(ray, normalize(rows[3].xyz)));
}

// Move raster depth to the nearest possible relief, leaving XY and interpolation
// on the card. Fragments add the unsampled remainder in linear view depth.
fn scene_relief_raster(clip: vec4<f32>, amount: f32) -> vec4<f32> {
    let rows = transpose(frame.view_projection);
    var axis = rows[3].xyz;
    if scene_orthographic() { axis = rows[2].xyz; }
    let offset = frame.view_projection * vec4<f32>(-normalize(axis) * amount, 0.0);
    let biased = clip + offset;
    var result = clip;
    result.z = 0.0;
    if biased.w > 0.0 { result.z = max(0.0, biased.z / biased.w) * clip.w; }
    return result;
}

fn scene_packed_atlas(input: VertexOutput) -> bool {
    return input.atlas_eye.w != 0.0 && (material.wind_clock.z & 2u) != 0u;
}
fn scene_atlas_first(map: texture_2d<f32>, footprint: AtlasFootprint) -> vec4<f32> {
    if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u {
        return textureSampleGrad(map, material_sampler, footprint.uv[0], footprint.dx[0], footprint.dy[0]);
    }
    return textureSampleLevel(map, material_sampler, footprint.uv[0], 0.0);
}
fn scene_surface_channels(input: VertexOutput) -> vec2<f32> {
    if (material.texture_flags & FLAG_ROUGH_METAL) == 0u { return vec2<f32>(material.roughness, material.metalness); }
    let footprint = scene_atlas_footprint(rough_metal_texture, input);
    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if scene_packed_atlas(input) { return scene_atlas_first(rough_metal_texture, footprint).rg; }
        }
    }
    let sampled = scene_atlas_sample(rough_metal_texture, input, footprint);
    return vec2<f32>(material.roughness * sampled.g, material.metalness * sampled.b);
}

fn scene_atlas_inset(dimensions: vec2<f32>) -> vec2<f32> {
    let border = vec2<f32>(material.impostor_grid.z);
    if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u { return border; }
    return max(border, 0.5 * material.impostor_grid.xy / dimensions);
}
