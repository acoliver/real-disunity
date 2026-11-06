struct UiDepthFrame {
    clip_to_view: mat4x4<f32>,
    output_size: vec4<f32>,
}
@group(0) @binding(0) var<uniform> ui_depth_frame: UiDepthFrame;
@group(0) @binding(1) var ui_scene_depth: texture_2d<f32>;
// One millimetre in view space keeps surface annotations visible without a distance-dependent bias.
const UI_DEPTH_BIAS_METRES: f32 = 0.001;
fn test_ui_depth(input: UiOverlayVertex) {
    let uv = input.position.xy / ui_depth_frame.output_size.xy;
    let size = textureDimensions(ui_scene_depth);
    let pixel = min(vec2<u32>(uv * vec2<f32>(size)), size - vec2<u32>(1u));
    let scene_depth = textureLoad(ui_scene_depth, vec2<i32>(pixel), 0).r;
    let view = ui_depth_frame.clip_to_view * vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), input.depth, 1.0);
    if -view.z / view.w > scene_depth + UI_DEPTH_BIAS_METRES { discard; }
}
