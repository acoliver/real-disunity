struct WaterFrame {
    clip: mat4x4<f32>, view: mat4x4<f32>, inverse_clip: mat4x4<f32>,
    origin_time: vec4<f32>, extent: vec4<f32>, color_roughness: vec4<f32>, absorption: vec4<f32>,
    sun_direction: vec4<f32>, sun_radiance: vec4<f32>, ambient: vec4<f32>,
}
@group(0) @binding(0) var<uniform> water: WaterFrame;
@group(0) @binding(1) var source_color: SOURCE_COLOR;
@group(0) @binding(2) var source_depth: SOURCE_DEPTH;
@vertex fn vs_snapshot(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[index], 0.0, 1.0);
}
@fragment fn fs_color(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    var color = vec4<f32>(0.0);
    for (var sample_index = 0; sample_index < SAMPLE_COUNT; sample_index += 1) { color += COLOR_LOAD; }
    return color / f32(SAMPLE_COUNT);
}
// The frame's depth is not reversed: 0 is the near plane and 1 the far plane and the cleared
// value, so the nearest sample of a pixel is the smallest.
fn nearest_depth(pixel: vec2<i32>) -> f32 {
    var depth = 1.0;
    for (var sample_index = 0; sample_index < SAMPLE_COUNT; sample_index += 1) { depth = min(depth, DEPTH_LOAD); }
    return depth;
}
fn view_metres(p: vec4<f32>, depth: f32) -> f32 {
    let uv = p.xy / vec2<f32>(textureDimensions(source_depth));
    let world = water.inverse_clip * vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), depth, 1.0);
    return -(water.view * (world / world.w)).z;
}
@fragment fn fs_depth(@builtin(position) p: vec4<f32>) -> @location(0) f32 {
    return view_metres(p, nearest_depth(vec2<i32>(p.xy)));
}
struct DepthPair { @location(0) metres: f32, @location(1) device: f32 }
@fragment fn fs_depth_device(@builtin(position) p: vec4<f32>) -> DepthPair {
    let depth = nearest_depth(vec2<i32>(p.xy));
    return DepthPair(view_metres(p, depth), depth);
}
