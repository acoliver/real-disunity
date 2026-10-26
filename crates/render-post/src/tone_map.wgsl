@group(0) @binding(0) var source: texture_2d<f32>;
override EXPOSURE: f32 = 1.0;
override CURVE: u32 = 0u;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var output: VertexOutput;
    output.position = vec4<f32>(positions[index], 0.0, 1.0);
    output.uv = positions[index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return output;
}
// Classify bits before arithmetic: NaN and negative radiance map to zero;
// positive infinity maps to the curve's limit. Zero exposure always gives zero.
// WGSL does not guarantee NaN or infinity preservation through texture loads or arithmetic.
// This policy is verified on Metal and the lavapipe Vulkan CI job, not guaranteed by WGSL.
fn tone_map(radiance: f32) -> f32 {
    let bits = bitcast<u32>(radiance);
    if EXPOSURE == 0.0 || (bits & 0x80000000u) != 0u || (bits & 0x7fffffffu) > 0x7f800000u {
        return 0.0;
    }
    if bits == 0x7f800000u {
        return 1.0;
    }
    let exposed = radiance * EXPOSURE;
    if bitcast<u32>(exposed) == 0x7f800000u {
        return 1.0;
    }
    if CURVE == 1u { return aces_display(exposed); }
    return exposed / (1.0 + exposed);
}
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = textureDimensions(source);
    let texel = min(vec2<u32>(input.uv * vec2<f32>(size)), size - vec2<u32>(1));
    let color = textureLoad(source, vec2<i32>(texel), 0);
    if CURVE == 2u {
        var v = vec3<f32>(0.0);
        if EXPOSURE != 0.0 { v = vec3<f32>(positive_hdr(color.r),positive_hdr(color.g),positive_hdr(color.b)) * EXPOSURE; }
        return vec4<f32>(hill_display(v),color.a);
    }
    return vec4<f32>(tone_map(color.r), tone_map(color.g), tone_map(color.b), color.a);
}
