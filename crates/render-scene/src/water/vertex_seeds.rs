//! Vertex seeds reuse the shader's explicit mip-zero addressing and filtering.
use crate::shader::rewrite::ShaderRewrite;
use worldrunner_render_plan::{SceneWaterBody, SceneWaterSeedSampling};

pub(super) const SPECIALIZATION: u32 = 512;

pub(super) fn key(body: &SceneWaterBody) -> u32 {
    let endpoint = u32::from(super::endpoint::enabled(body)) * super::endpoint::SPECIALIZATION;
    let vertex = u32::from(body.material.flow_seed_sampling == SceneWaterSeedSampling::Vertex)
        * SPECIALIZATION;
    endpoint | vertex
}

pub(super) fn specialize(source: &str, terms: u32) -> std::borrow::Cow<'_, str> {
    let addressed = super::endpoint::specialize(source, terms);
    if terms & SPECIALIZATION == 0 {
        return addressed;
    }
    std::borrow::Cow::Owned(self::source(
        &addressed,
        terms & super::endpoint::SPECIALIZATION != 0,
    ))
}

pub(super) fn source(source: &str, endpoint: bool) -> String {
    let sampling = if endpoint {
        "output.flow_seed = endpoint_seed(output.seed);".to_owned()
    } else {
        repeat_sampling(source)
    };
    let source = vertex_texels(source);
    let source = source
        .anchored("water::vertex_seeds::source", 1, "@location(1) wind: vec2<f32>,", "@location(1) wind: vec2<f32>, @location(3) @interpolate(perspective, sample) flow_seed: vec2<f32>,")
        .anchored("water::vertex_seeds::source", 1, "return output;", &format!("{sampling}\n    return output;"));
    let (vertex, fragment) = source
        .split_once("@fragment fn fs_water")
        .expect("water entry point");
    // The water fragment samples the flow twice; probe sources add their own reads.
    let fragment = fragment
        .anchored_at_least(
            "water::vertex_seeds::source",
            2,
            "flow_velocity(flow_uv)",
            "(input.flow_seed * 2.0 - 1.0)",
        )
        .anchored(
            "water::vertex_seeds::source",
            1,
            "(textureSampleLevel(surface_flow, surface_sampler, flow_uv, 0.0).rg * 2.0 - 1.0)",
            "(input.flow_seed * 2.0 - 1.0)",
        );
    format!("{vertex}@fragment fn fs_water{fragment}")
}

// Generate both stages from the shader's single coordinate expression.
fn repeat_sampling(source: &str) -> String {
    let start = source.find("    var flow_uv =").expect("seed coordinates");
    let end = source[start..]
        .find("    if water.surface.w")
        .expect("seed sampling");
    let coordinates = source[start..start + end]
        .anchored(
            "water::vertex_seeds::repeat_sampling",
            2,
            "input.world",
            "world",
        )
        .anchored(
            "water::vertex_seeds::repeat_sampling",
            2,
            "flow_uv",
            "seed_uv",
        );
    format!("{coordinates}    output.flow_seed = bilinear_rg(surface_flow, seed_uv);")
}

// Texel-centre fetches preserve the shared float weights on downlevel samplers.
fn vertex_texels(source: &str) -> String {
    let source = source.anchored(
        "water::vertex_seeds::vertex_texels",
        1,
        "textureLoad(source, wrapped, 0).rg",
        "water_seed_texel(source, wrapped)",
    );
    let source = source
        .anchored(
            "water::vertex_seeds::vertex_texels",
            1,
            "textureLoad(surface_flow, base, 0).rg",
            "water_seed_texel(surface_flow, base)",
        )
        .anchored(
            "water::vertex_seeds::vertex_texels",
            1,
            "textureLoad(surface_flow, vec2<i32>(next.x, base.y), 0).rg",
            "water_seed_texel(surface_flow, vec2<i32>(next.x, base.y))",
        )
        .anchored(
            "water::vertex_seeds::vertex_texels",
            1,
            "textureLoad(surface_flow, vec2<i32>(base.x, next.y), 0).rg",
            "water_seed_texel(surface_flow, vec2<i32>(base.x, next.y))",
        )
        .anchored(
            "water::vertex_seeds::vertex_texels",
            1,
            "textureLoad(surface_flow, next, 0).rg",
            "water_seed_texel(surface_flow, next)",
        );
    format!(
        "{source}\nfn water_seed_texel(source: texture_2d<f32>, pixel: vec2<i32>) -> vec2<f32> {{
        let uv = (vec2<f32>(pixel) + 0.5) / vec2<f32>(textureDimensions(source));
        return textureSampleLevel(source, surface_sampler, uv, 0.0).rg;
    }}"
    )
}
