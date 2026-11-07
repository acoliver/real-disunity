override UI_WIDTH: f32;
override UI_HEIGHT: f32;
struct UiOverlayVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) depth: f32,
}
UI_DEPTH_BINDINGS
@vertex
fn vs_ui_overlay(vertex: VertexInput, tint: InstanceTint) -> UiOverlayVertex {
    var out: UiOverlayVertex;
    out.position = vec4<f32>(vertex.position.xy * 2.0 / vec2<f32>(UI_WIDTH, UI_HEIGHT) - 1.0, 0.0, 1.0);
    out.depth = vertex.position.z;
    out.uv = vertex.texcoord;
    out.tint = tint.tint;
    return out;
}
@fragment
fn fs_ui_overlay(input: UiOverlayVertex) -> @location(0) vec4<f32> {
    let colour = textureSample(albedo_texture, material_sampler, input.uv) * input.tint;
    UI_DEPTH_TEST
    return colour;
}
