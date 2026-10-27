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
    target: (u32, &[(&str, f64)]),
) -> (RenderPipeline, BindGroupLayout, RenderPipeline) {
    let (source, layered) = source;
    let source = if layered {
        layered_source(source)
    } else {
        source.to_owned()
    };
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
    let source = source.replace(
        "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);",
        "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
    ) + include_str!("../terrain/pages/sampling.wgsl");
    let pipeline = build_terrain(device, &pipeline_layout, &source, buffers, targets, target);
    (terrain, layout, pipeline)
}

fn layered_source(source: &str) -> String {
    let shared = include_str!("../terrain/pages/layered_gpu.wgsl")
        .replace("@group(0) @binding(0)", "@group(1) @binding(8)")
        .replace("@group(0) @binding(1)", "@group(1) @binding(9)")
        .replace("@group(0) @binding(2)", "@group(1) @binding(10)");
    let source = source.replace("    var accumulator: BlendAccumulator;", "    if terrain.dimensions_and_count.w != 0u {\n        let sample = compose_layered(world_xy, world_xy_dx, world_xy_dy);\n        return BlendedSurface(clamp(sample.albedo, vec3<f32>(0.0), vec3<f32>(1.0)), sample.normal, clamp(sample.roughness, 0.0, 1.0), clamp(sample.metallic, 0.0, 1.0));\n    }\n    var accumulator: BlendAccumulator;");
    format!("{source}\n{shared}")
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
    target: (u32, &[(&str, f64)]),
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
    target: (u32, &[(&str, f64)]),
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
