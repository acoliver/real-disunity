override UI_WIDTH: f32;
override UI_HEIGHT: f32;
struct UiOverlayVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) depth: f32,
}
struct UiDistanceStyle {
    spread: f32,
    radius: f32,
    mode: u32,
    // Nonzero when the texture holds colour multiplied by its alpha, as a layer atlas does.
    premultiplied: u32,
    effect: vec4<f32>,
}
@group(2) @binding(0) var<uniform> ui_distance: UiDistanceStyle;
UI_DEPTH_BINDINGS
@vertex
fn vs_ui_overlay(vertex: VertexInput, tint: InstanceTint, @location(12) corner_color: vec4<f32>) -> UiOverlayVertex {
    var out: UiOverlayVertex;
    out.position = vec4<f32>(vertex.position.xy * 2.0 / vec2<f32>(UI_WIDTH, UI_HEIGHT) - 1.0, 0.0, 1.0);
    out.depth = vertex.position.z;
    out.uv = vertex.texcoord;
    out.tint = tint.tint * corner_color;
    return out;
}
fn distance_colour(alpha: f32, tint: vec4<f32>, gx: vec2<f32>, gy: vec2<f32>) -> vec4<f32> {
    // Geometric derivatives stay defined at medial axes and constant/saturated samples.
    let t = dot(gx, gx) + dot(gy, gy);
    let determinant = gx.x * gy.y - gx.y * gy.x;
    let sigma = sqrt(0.5 * (t + sqrt(max(t * t - 4.0 * determinant * determinant, 0.0))));
    let distance = (alpha - 0.5) * 2.0 * ui_distance.spread / sigma;
    let fill = smoothstep(-0.5, 0.5, distance);
    var effect = 0.0;
    if ui_distance.mode == 2u {
        effect = max(smoothstep(-ui_distance.radius - 0.5, -ui_distance.radius + 0.5, distance) - fill, 0.0);
    } else if ui_distance.mode == 3u {
        effect = smoothstep(-ui_distance.radius, 0.0, distance) * (1.0 - fill);
    }
    let opacity = tint.a * fill + ui_distance.effect.a * effect;
    let rgb = tint.rgb * tint.a * fill + ui_distance.effect.rgb * ui_distance.effect.a * effect;
    if opacity == 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(rgb / opacity, opacity);
}
@fragment
fn fs_ui_overlay(input: UiOverlayVertex) -> @location(0) vec4<f32> {
    var sample = textureSample(albedo_texture, material_sampler, input.uv);
    if ui_distance.premultiplied != 0u {
        sample = vec4<f32>(sample.rgb / max(sample.a, 1.0e-6), sample.a);
    }
    let texel = input.uv * vec2<f32>(textureDimensions(albedo_texture));
    let gx = dpdx(texel);
    let gy = dpdy(texel);
    var colour = sample * input.tint;
    if ui_distance.mode != 0u {
        colour = distance_colour(sample.a, input.tint, gx, gy);
    }
    UI_DEPTH_TEST
    return colour;
}
