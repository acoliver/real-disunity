use super::{
    BindGroupLayout, ColorTargetState, Device, PipelineLayoutDescriptor, RenderPipeline,
    VertexBufferLayout, build_terrain,
};

pub(super) fn build_cached(
    device: &Device,
    groups: &[Option<&BindGroupLayout>],
    source: &str,
    buffers: &[Option<VertexBufferLayout<'_>>],
    targets: &[Option<ColorTargetState>],
    samples: u32,
) -> (BindGroupLayout, RenderPipeline) {
    let layout = crate::upload::cached_terrain_bind_group_layout(device);
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
    let pipeline = build_terrain(device, &pipeline_layout, &source, buffers, targets, samples);
    (layout, pipeline)
}
