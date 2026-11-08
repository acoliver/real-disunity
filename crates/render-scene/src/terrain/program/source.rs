//! The WGSL of a terrain program, composed from the context's base source and the surface
//! source the draw selects.
//!
//! The composition is textual and exact: the byte-equality captures compare the pixels it
//! produces with the frozen path's, and `draw/tests/metadata_forward_sources.rs` fingerprints
//! the composed sources.

use super::Surface;

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

pub(crate) fn cached_source(source: &str) -> String {
    // Cached pages already contain surface selection; leave its source grid unbound.
    let source = source.replace(
        "let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));",
        "let centre_surface = 0u;",
    );
    source.replace(
        "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);",
        "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
    ) + SAMPLING
}

pub(crate) fn layered_source(source: &str) -> String {
    let mut shared = LAYERED.to_owned();
    for (input, output) in (0..3)
        .map(|i| (i, i + 8))
        .chain((5..17).map(|i| (i, i + 6)))
    {
        shared = shared.replace(
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
    format!(
        "{source}\nfn layered_gradient_scale() -> f32 {{ return exp2(frame.lighting_options.w); }}\n{shared}"
    )
}
