struct ReflectionCamera {
    projection: mat4x4<f32>, inverse_projection: mat4x4<f32>,
    settings: vec4<f32>, size: vec4<f32>,
    traversal: vec4<f32>, limits: vec4<f32>, behind: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: ReflectionCamera;
@group(0) @binding(1) var opaque: texture_2d<f32>;
@group(0) @binding(2) var normals: texture_2d<f32>;
@group(0) @binding(3) var materials: texture_2d<f32>;
@group(0) @binding(4) var depth_0: texture_2d<f32>;
@group(0) @binding(5) var depth_2: texture_2d<f32>;
@group(0) @binding(6) var depth_4: texture_2d<f32>;
@group(0) @binding(7) var depth_6: texture_2d<f32>;
@group(0) @binding(8) var receiver_depth: texture_2d<f32>;
@group(0) @binding(9) var opaque_forward_depth: texture_2d<f32>;
fn depth_at(level: u32, uv: vec2<f32>) -> f32 {
    let scale = exp2(min(f32(level*2u), ceil(log2(max(camera.size.x,camera.size.y)))));
    let coordinate = vec2<i32>(uv*camera.size.xy/scale);
    switch level {
        case 0u: { return textureLoad(depth_0, coordinate, 0).r; }
        case 1u: { return textureLoad(depth_2, coordinate, 0).r; }
        case 2u: { return textureLoad(depth_4, coordinate, 0).r; }
        default: { return textureLoad(depth_6, coordinate, 0).r; }
    }
}
fn edge_fade(uv: vec2<f32>) -> f32 {
    let edge = min(min(uv.x, uv.y), min(1.0-uv.x, 1.0-uv.y));
    return smoothstep(0.0, 0.08, edge);
}
fn matches_receiver_depth(surface: f32, depth: f32, normal: vec3<f32>, pixel: vec2<i32>) -> bool {
    // Water-covered targets cannot supply the immutable opaque radiance.
    // WATER_TARGET_OWNERSHIP
    let opaque_depth = textureLoad(opaque_forward_depth, pixel, 0).r;
    return surface == opaque_depth;
}
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Trace targets can be half resolution, with ceil-divided odd extents.
    let trace_size = ceil(camera.size.xy / camera.size.z);
    let sample = u32(pixel.x / trace_size.x);
    var origin_uv = vec2<f32>(pixel.x - f32(sample) * trace_size.x, pixel.y) / trace_size;
    let p = clamp(vec2<i32>(origin_uv * camera.size.xy), vec2<i32>(0), vec2<i32>(camera.size.xy)-vec2<i32>(1));
    var packed = textureLoad(normals, p, 0);
    var roughness = textureLoad(materials, p, 0).a;
    var origin_depth = textureLoad(receiver_depth, p, 0).r;
    // WATER_RECEIVER_SELECTION
    if packed.a == 0.0 || roughness >= camera.settings.z || camera.settings.w != 0.0 { return vec4<f32>(0.0); }
    let centre = reflection_position(origin_uv, origin_depth);
    let normal = reflection_normal(packed.xyz);
    let direction = reflect(reflection_incident(origin_uv), normal);
    if camera.limits.z != 0.0 { return grid_reflection(centre, direction, origin_uv, roughness); }
    let start = centre + normal * 0.005;
    var distance = camera.traversal.x;
    let near_point = camera.inverse_projection * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let near_z = near_point.z / near_point.w;
    if start.z > near_z { return vec4<f32>(0.0); }
    if direction.z > 0.0 { distance = min(distance, max(0.0, (near_z-start.z)/direction.z)); }
    let end = start + direction * distance;
    let a = camera.projection * vec4<f32>(start, 1.0);
    let b = camera.projection * vec4<f32>(end, 1.0);
    if a.w <= 0.0 || b.w <= 0.0 { return vec4<f32>(0.0); }
    let uv_a = a.xy/a.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let uv_b = b.xy/b.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let span = length((uv_b-uv_a) * camera.size.xy);
    if span < 2.0 { return vec4<f32>(0.0); }
    let stride = 1.0/span;
    var t = camera.traversal.z * stride;
    var remaining = reflection_pass_count(roughness, camera.traversal.w, camera.behind);
    var level = 0u;
    for (var step = 0u; step < u32(camera.settings.x); step++) {
        if t >= 1.0 { break; }
        let sample_uv = mix(uv_a, uv_b, t);
        if any(sample_uv <= vec2<f32>(0.0)) || any(sample_uv >= vec2<f32>(1.0)) { break; }
        let point = mix(start/a.w, end/b.w, t) / mix(1.0/a.w, 1.0/b.w, t);
        let scale = exp2(min(f32(level*2u), ceil(log2(max(camera.size.x,camera.size.y)))));
        let cell = floor(sample_uv*camera.size.xy/scale);
        let delta = (uv_b-uv_a)*camera.size.xy;
        let boundary = (cell + select(vec2<f32>(0.0), vec2<f32>(1.0), delta > vec2<f32>(0.0))) * scale;
        let crossing = (boundary - uv_a*camera.size.xy) / select(delta, vec2<f32>(1e-20), abs(delta) < vec2<f32>(1e-20));
        let exit_t = min(min(select(1e20, crossing.x, crossing.x > t+1e-7), select(1e20, crossing.y, crossing.y > t+1e-7)), 1.0);
        let next_t = max(t+stride*0.001, exit_t);
        let next_point = mix(start/a.w, end/b.w, next_t) / mix(1.0/a.w, 1.0/b.w, next_t);
        let surface_depth = depth_at(level, sample_uv);
        let difference = -point.z - surface_depth;
        let next_difference = -next_point.z - surface_depth;
        if max(difference,next_difference) >= 0.0 {
            if level > 0u { level--; continue; }
            let thickness = reflection_thickness(camera.settings.y, camera.traversal.y, surface_depth);
            if min(difference, next_difference) > thickness {
                if remaining == 0u { break; }
                remaining--;
                t = next_t + stride*0.001;
                continue;
            }
            let hit_uv = sample_uv;
            let hit_pixel = vec2<i32>(hit_uv * camera.size.xy);
            let hit_normal = textureLoad(normals, hit_pixel, 0);
            let metadata_depth = textureLoad(receiver_depth, hit_pixel, 0).r;
            // Opaque radiance and normals cannot describe a different final surface.
            if !matches_receiver_depth(surface_depth, metadata_depth, hit_normal.xyz, hit_pixel) { break; }
            if hit_normal.a == 0.0 || dot(hit_normal.xyz, direction) >= 0.0 { break; }
            var hit = textureLoad(opaque, hit_pixel, 0).rgb;
            if camera.limits.y > 0.0 {
                hit = bounded_radiance(hit, camera.limits.y);
            }
            let budget_fade = smoothstep(0.0, 0.2, 1.0-f32(step)/camera.settings.x);
            let thickness_fade = 1.0-smoothstep(thickness*0.5, thickness, max(difference,0.0));
            let confidence = edge_fade(hit_uv) * edge_fade(origin_uv) * budget_fade * thickness_fade;
            return vec4<f32>(hit * confidence, confidence);
        }
        t = next_t + stride*0.001;
        level = min(level+1u, 3u);
    }
    return vec4<f32>(0.0);
}

struct GridIntersection {
    pixel: vec2<f32>, depth: f32, steps: u32, levels: u32,
}
fn grid_depth(level: u32, pixel: vec2<f32>, size: vec2<f32>) -> f32 {
    let coordinate = vec2<i32>(pixel / exp2(f32(level)));
    switch level {
        case 0u: { return textureLoad(depth_0, clamp(coordinate, vec2<i32>(0), vec2<i32>(textureDimensions(depth_0))-vec2<i32>(1)), 0).r; }
        case 1u: { return textureLoad(depth_2, clamp(coordinate, vec2<i32>(0), vec2<i32>(textureDimensions(depth_2))-vec2<i32>(1)), 0).r; }
        case 2u: { return textureLoad(depth_4, clamp(coordinate, vec2<i32>(0), vec2<i32>(textureDimensions(depth_4))-vec2<i32>(1)), 0).r; }
        default: { return textureLoad(depth_6, clamp(coordinate, vec2<i32>(0), vec2<i32>(textureDimensions(depth_6))-vec2<i32>(1)), 0).r; }
    }
}
fn grid_walk(first: vec2<f32>, last: vec2<f32>, weighted_depth: vec2<f32>, reciprocal_w: vec2<f32>,
    end_fraction: f32, size: vec2<f32>, skip: f32, budget: u32, width: vec2<f32>, allowance: u32) -> GridIntersection {
    var result = GridIntersection(vec2<f32>(0.0), -1.0, 0u, 0u);
    var finish = last;
    if dot(last-first, last-first) < 0.0001 { finish += vec2<f32>(0.01); }
    let vertical = abs(finish.x-first.x) < abs(finish.y-first.y);
    let origin = select(first, first.yx, vertical);
    let delta = select(finish-first, (finish-first).yx, vertical);
    let dimensions = select(size, size.yx, vertical);
    let orientation = sign(delta.x);
    let increment = vec4<f32>(1.0, delta.y/delta.x,
        (weighted_depth.y-weighted_depth.x)/delta.x, (reciprocal_w.y-reciprocal_w.x)/delta.x);
    let initial = vec4<f32>(origin, weighted_depth.x, reciprocal_w.x);
    let edge = select(vec2<f32>(0.0), dimensions, delta > vec2<f32>(0.0));
    let moving = delta != vec2<f32>(0.0);
    let screen_end = select(vec2<f32>(1e30), (edge-origin)/select(vec2<f32>(1.0), delta, moving), moving);
    let limit = min(end_fraction, min(screen_end.x, screen_end.y));
    let boundary_offset = select(0.0, 1.0, delta.x > 0.0);
    let terminal = floor((origin.x + limit*delta.x)*orientation) + boundary_offset;
    let begin = floor(origin.x + skip*orientation) + boundary_offset;
    var cursor = initial + increment*(begin-origin.x);
    cursor.z /= cursor.w;
    var level = 0u;
    let highest = min(3u, u32(ceil(log2(max(size.x, size.y)))));
    var remaining = allowance;
    for (var iteration = 0u; iteration < budget; iteration++) {
        if cursor.x*orientation >= terminal { break; }
        result.steps++;
        result.levels = (result.levels*5u + level+1u) % 1021u;
        let footprint = exp2(f32(level));
        let boundary = (floor((cursor.x+0.5*orientation)/footprint)+boundary_offset)*footprint;
        let next_axis = min(boundary*orientation, terminal)*orientation;
        var next = initial + increment*(next_axis-origin.x);
        next.z /= next.w;
        let lower = min(cursor.z, next.z);
        let upper = max(cursor.z, next.z);
        let endpoint_a = vec2<f32>(cursor.x+0.001*orientation, cursor.y);
        let endpoint_b = vec2<f32>(next.x-0.001*orientation, next.y);
        let surface = min(grid_depth(level, select(endpoint_a, endpoint_a.yx, vertical), size),
            grid_depth(level, select(endpoint_b, endpoint_b.yx, vertical), size));
        if surface < upper && level == 0u {
            let thickness = width.x + surface*width.y;
            if lower-thickness <= surface {
                let hit = cursor.xy + 0.5*orientation*increment.xy;
                result.pixel = select(hit, hit.yx, vertical);
                result.depth = surface;
                return result;
            }
            if remaining == 0u { return result; }
            remaining--;
        } else if surface < upper {
            level--;
            next = cursor;
        } else {
            level = min(level+1u, highest);
        }
        cursor = next;
    }
    return result;
}
fn grid_reflection(origin: vec3<f32>, direction: vec3<f32>, origin_uv: vec2<f32>, roughness: f32) -> vec4<f32> {
    let near_h = camera.inverse_projection * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let far_h = camera.inverse_projection * vec4<f32>(0.0, 0.0, 1.0, 1.0);
    let near_depth = -near_h.z/near_h.w;
    let far_depth = -far_h.z/far_h.w;
    if -origin.z < near_depth+0.0001 || -origin.z > far_depth-0.01 { return vec4<f32>(0.0); }
    var distance = camera.traversal.x;
    if -origin.z-direction.z*distance < near_depth+0.0001 { distance = (-near_depth-origin.z)/direction.z; }
    let finish = origin + direction*distance;
    let clip_a = camera.projection * vec4<f32>(origin, 1.0);
    let clip_b = camera.projection * vec4<f32>(finish, 1.0);
    let reciprocal = vec2<f32>(1.0/clip_a.w, 1.0/clip_b.w);
    let first = (clip_a.xy*reciprocal.x*vec2<f32>(0.5, -0.5)+vec2<f32>(0.5))*camera.size.xy;
    let last = (clip_b.xy*reciprocal.y*vec2<f32>(0.5, -0.5)+vec2<f32>(0.5))*camera.size.xy;
    let weighted = -vec2<f32>(origin.z, finish.z)*reciprocal;
    let clip_depth = select(near_depth+0.0001, far_depth-0.01, finish.z < origin.z);
    var depth_limit = 1.0;
    if finish.z != origin.z {
        depth_limit = min(1.0, (clip_depth*reciprocal.x-weighted.x)/
            (weighted.y-weighted.x-clip_depth*(reciprocal.y-reciprocal.x)));
    }
    let intersection = grid_walk(first, last, weighted, reciprocal, depth_limit, camera.size.xy,
        camera.traversal.z, u32(camera.settings.x), vec2<f32>(camera.settings.y, camera.traversal.y),
        reflection_pass_count(roughness, camera.traversal.w, camera.behind));
    if intersection.depth < 0.0 || any(intersection.pixel < vec2<f32>(0.0)) || any(intersection.pixel >= camera.size.xy) { return vec4<f32>(0.0); }
    let hit_pixel = vec2<i32>(intersection.pixel);
    let hit_normal = textureLoad(normals, hit_pixel, 0);
    let metadata_depth = textureLoad(receiver_depth, hit_pixel, 0).r;
    if !matches_receiver_depth(intersection.depth, metadata_depth, hit_normal.xyz, hit_pixel) { return vec4<f32>(0.0); }
    if hit_normal.a == 0.0 || dot(hit_normal.xyz, direction) >= 0.0 { return vec4<f32>(0.0); }
    var hit = textureLoad(opaque, hit_pixel, 0).rgb;
    if camera.limits.y > 0.0 { hit = bounded_radiance(hit, camera.limits.y); }
    let confidence = edge_fade(intersection.pixel/camera.size.xy) * edge_fade(origin_uv)
        * smoothstep(0.0, 0.2, 1.0-f32(intersection.steps-1u)/camera.settings.x);
    return vec4<f32>(hit*confidence, confidence);
}
