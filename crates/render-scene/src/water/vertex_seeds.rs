//! Vertex seeds reuse the shader's explicit mip-zero addressing and filtering.
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
        .replace("@location(1) wind: vec2<f32>,", "@location(1) wind: vec2<f32>, @location(3) @interpolate(perspective, sample) flow_seed: vec2<f32>,")
        .replace("return output;", &format!("{sampling}\n    return output;"));
    let (vertex, fragment) = source
        .split_once("@fragment fn fs_water")
        .expect("water entry point");
    let fragment = fragment
        .replace("flow_velocity(flow_uv)", "(input.flow_seed * 2.0 - 1.0)")
        .replace(
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
        .replace("input.world", "world")
        .replace("flow_uv", "seed_uv");
    format!("{coordinates}    output.flow_seed = bilinear_rg(surface_flow, seed_uv);")
}

// Texel-centre fetches preserve the shared float weights on downlevel samplers.
fn vertex_texels(source: &str) -> String {
    let source = source.replace(
        "textureLoad(source, wrapped, 0).rg",
        "water_seed_texel(source, wrapped)",
    );
    let source = source
        .replace(
            "textureLoad(surface_flow, base, 0).rg",
            "water_seed_texel(surface_flow, base)",
        )
        .replace(
            "textureLoad(surface_flow, vec2<i32>(next.x, base.y), 0).rg",
            "water_seed_texel(surface_flow, vec2<i32>(next.x, base.y))",
        )
        .replace(
            "textureLoad(surface_flow, vec2<i32>(base.x, next.y), 0).rg",
            "water_seed_texel(surface_flow, vec2<i32>(base.x, next.y))",
        )
        .replace(
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
