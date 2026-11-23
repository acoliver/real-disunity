//! The WGSL of a terrain program, composed from the context's base source and the surface
//! source the draw selects.
//!
//! The composition is textual and exact: the terrain captures print a hash of the pixels it
//! produces, and `draw/tests/metadata_forward_sources.rs` validates the composed sources with
//! naga.

use super::Surface;
use crate::shader::rewrite::ShaderRewrite;

const FOOTPRINT: &str = include_str!("../pages/footprint.wgsl");
const SAMPLING: &str = include_str!("../pages/sampling.wgsl");
const LAYERED: &str = include_str!("../pages/layered_gpu.wgsl");

/// The source of `surface` over `base`, the terrain source of the context: the footprint
/// functions in front, the layered source composer in place of the palette blend for layered
/// surfaces, and the page sampler behind, with surface selection replaced by the baked pages,
/// for page surfaces.
pub(crate) fn compose(surface: Surface, base: &str) -> String {
    let source = if surface.layered() {
        layered_source(base)
    } else {
        base.to_owned()
    };
    let source = FOOTPRINT.to_owned() + &source;
    if surface.pages() {
        cached_source(&source)
    } else {
        source
    }
}

/// The composed source of each surface over `base`, in the order plain, pages, layered, layered
/// pages, for tests that check every source a context can draw.
#[cfg(test)]
pub(crate) fn test_sources(base: &str) -> [String; 4] {
    use super::Surface;
    [
        Surface::Plain,
        Surface::Pages,
        Surface::Layered,
        Surface::LayeredPages,
    ]
    .map(|surface| compose(surface, base))
}

pub(crate) fn cached_source(source: &str) -> String {
    // Cached pages already contain surface selection; leave its source grid unbound.
    let source = source.anchored(
        "terrain::program::source::cached_source",
        1,
        "let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));",
        "let centre_surface = 0u;",
    );
    source.anchored(
        "terrain::program::source::cached_source",
        1,
        "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);",
        "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
    ) + SAMPLING
}

pub(crate) fn layered_source(source: &str) -> String {
    let mut shared = LAYERED.to_owned();
    // The layered composer declares bindings 0 to 2, 5 to 11, 15 and 16 of set 0.
    for (input, output) in (0..3)
        .map(|i| (i, i + 8))
        .chain((5..12).chain(15..17).map(|i| (i, i + 6)))
    {
        shared = shared.anchored(
            "terrain::program::source::layered_source",
            1,
            &format!("@group(0) @binding({input})"),
            &format!("@group(1) @binding({output})"),
        );
    }
    // Layered draws use their source composer exclusively. Omitting the unused
    // conventional palettes leaves room for decals within the baseline stage limit.
    let start = source
        .find("fn blend_surfaces(")
        .expect("terrain blend function");
    let end = source[start..]
        .find("\nstruct TerrainVertexInput")
        .map(|offset| start + offset)
        .expect("terrain vertex entry");
    let source = format!(
        "{}fn blend_surfaces(grid: vec2<f32>, world_xy: vec2<f32>, world_xy_dx: vec2<f32>, world_xy_dy: vec2<f32>, centre_surface: u32) -> BlendedSurface {{ let sample = compose_layered(world_xy, world_xy_dx, world_xy_dy); return BlendedSurface(clamp(sample.albedo, vec3<f32>(0.0), vec3<f32>(1.0)), sample.normal, clamp(sample.roughness, 0.0, 1.0), clamp(sample.metallic, 0.0, 1.0)); }}\n{}",
        &source[..start],
        &source[end..]
    );
    // The frame's material mip bias finer-selects the screen-space gradients. The page bake
    // defines the same function as 1.0 because its footprints are not screen footprints.
    // A fragment composes every layer; only the bake's workgroups can share a candidate set.
    format!(
        "{source}\nfn layered_gradient_scale() -> f32 {{ return exp2(frame.lighting_options.w); }}\nfn layered_next(index: u32) -> u32 {{ return index; }}\n{shared}"
    )
}
