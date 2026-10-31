use super::{
    BindGroupLayout, ColorTargetState, Device, PipelineLayoutDescriptor, RenderPipeline,
    VertexBufferLayout, build_terrain,
};

pub(super) fn build_cached(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: (&str, bool),
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], wgpu::FrontFace),
) -> (RenderPipeline, BindGroupLayout, RenderPipeline) {
    let (source, layered) = source;
    let source = if layered {
        layered_source(source)
    } else {
        source.to_owned()
    };
    let source = include_str!("../terrain/pages/footprint.wgsl").to_owned() + &source;
    let source = source.as_str();
    let terrain_layout = super::scene_layout(device, "terrain", groups);
    let terrain = build_terrain(device, &terrain_layout, source, buffers, targets, target);
    let layout = crate::upload::terrain_layout(device, true, layered);
    let mut groups = groups.to_vec();
    groups[1] = Some(&layout);
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("cached terrain"),
        bind_group_layouts: &groups,
        immediate_size: 0,
    });
    // Cached pages already contain surface selection; leave its source grid unbound.
    let source = source.replace(
        "let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));",
        "let centre_surface = 0u;",
    );
    let source = source.replace(
        "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);",
        "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
    ) + include_str!("../terrain/pages/sampling.wgsl");
    let pipeline = build_terrain(device, &pipeline_layout, &source, buffers, targets, target);
    (terrain, layout, pipeline)
}

fn layered_source(source: &str) -> String {
    let mut shared = include_str!("../terrain/pages/layered_gpu.wgsl").to_owned();
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

#[derive(Clone, Debug)]
pub(crate) struct LayeredPipelines {
    pub layout: BindGroupLayout,
    pub cached_layout: BindGroupLayout,
    pub terrain: RenderPipeline,
    pub cached: RenderPipeline,
}

pub(super) fn build_layered(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], wgpu::FrontFace),
) -> Option<LayeredPipelines> {
    if !crate::terrain::pages::layered_gpu::supported(&device.limits()) {
        return None;
    }
    let layout = crate::upload::terrain_layout(device, false, true);
    let mut groups = groups.to_vec();
    groups[1] = Some(&layout);
    let (terrain, cached_layout, cached) =
        build_cached(device, &groups, (source, true), buffers, targets, target);
    Some(LayeredPipelines {
        layout,
        cached_layout,
        terrain,
        cached,
    })
}

pub(super) fn build_terrain_set(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    target: (u32, &[(&str, f64)], wgpu::FrontFace),
) -> (
    RenderPipeline,
    BindGroupLayout,
    RenderPipeline,
    Option<LayeredPipelines>,
) {
    let (terrain, layout, cached) =
        build_cached(device, groups, (source, false), buffers, targets, target);
    let layered = build_layered(device, groups, source, buffers, targets, target);
    (terrain, layout, cached, layered)
}
