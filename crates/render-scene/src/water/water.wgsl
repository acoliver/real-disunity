struct WaterFrame {
    clip: mat4x4<f32>, view: mat4x4<f32>, inverse_clip: mat4x4<f32>,
    origin_time: vec4<f32>, extent: vec4<f32>, color_roughness: vec4<f32>, absorption: vec4<f32>,
    sun_direction: vec4<f32>, sun_radiance: vec4<f32>, ambient: vec4<f32>,
}
@group(0) @binding(0) var<uniform> water: WaterFrame;
@group(0) @binding(1) var opaque_color: texture_2d<f32>;
@group(0) @binding(2) var opaque_depth: texture_2d<f32>;
struct WaterVertex { @builtin(position) position: vec4<f32>, @location(0) world: vec3<f32>, }
@vertex fn vs_water(@builtin(vertex_index) index: u32) -> WaterVertex {
    let corners = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let xy = water.origin_time.xy + corners[index] * water.extent.xy;
    var output: WaterVertex;
    output.world = vec3<f32>(xy, water.origin_time.z);
    output.position = water.clip * vec4<f32>(output.world, 1.0);
    return output;
}
@fragment fn fs_water(input: WaterVertex) -> @location(0) vec4<f32> {
    let normal = normalize(vec3<f32>(sin(input.world.x * 1.7 + water.origin_time.w) * 0.08, cos(input.world.y * 2.1 + water.origin_time.w * 0.7) * 0.08, 1.0));
    let visibility = sunlight_visibility(input.world, normal);
    let size = vec2<i32>(textureDimensions(opaque_color));
    let original = clamp(vec2<i32>(input.position.xy), vec2<i32>(0), size - vec2<i32>(1));
    var refracted = clamp(original + vec2<i32>(normal.xy * 24.0 * (1.0 - water.color_roughness.w)), vec2<i32>(0), size - vec2<i32>(1));
    let surface_depth = -(water.view * vec4<f32>(input.world, 1.0)).z;
    if textureLoad(opaque_depth, refracted, 0).r < surface_depth { refracted = original; }
    let distance = max(textureLoad(opaque_depth, refracted, 0).r - surface_depth, 0.0);
    let transmission = exp(-water.absorption.xyz * distance);
    let background = textureLoad(opaque_color, refracted, 0).rgb;
    let lighting = water.ambient.xyz + water.sun_radiance.xyz * max(dot(normal, water.sun_direction.xyz), 0.0) * visibility;
    let scattering = water.color_roughness.xyz * lighting;
    return vec4<f32>(background * transmission + scattering * (vec3<f32>(1.0) - transmission), 1.0);
}
