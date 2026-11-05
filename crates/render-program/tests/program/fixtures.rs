//! WGSL stand-ins for a Unity pass: one vertex stage and fragment variants.

/// Material constants (binding 0, used by both stages), a texture and sampler (1, 2) and the
/// vertex-only draw constants (3).
fn header() -> &'static str {
    "
struct Material { tint: vec4<f32>, offset: vec4<f32> }
struct Draw { scale: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var albedo: texture_2d<f32>;
@group(0) @binding(2) var albedo_sampler: sampler;
@group(0) @binding(3) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(pos * draw.scale.xy, material.offset.w, 1.0);
    out.uv = uv;
    return out;
}
"
}

/// The vertex stage and a fragment stage whose color is `expression` of `texel`.
pub fn textured(expression: &str) -> String {
    format!(
        "{}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {{
    let texel = textureSample(albedo, albedo_sampler, in.uv);
    return {expression};
}}",
        header()
    )
}
