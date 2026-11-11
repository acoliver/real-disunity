use wgpu::{
    BindGroup, BindGroupLayout, Buffer, Device, Face, IndexFormat, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, RenderPipeline, VertexState,
};

use super::pipelines::{scene_shader, vertex_buffers};
use super::{Chunk, Commands, Geometry};
use crate::draw::DrawCount;
use crate::geometry::{INSTANCE_STRIDE, instance_attributes, vertex_attributes};

/// `shadows::resources::terrain_depth_pipeline` of the fixed shadow caster, with its shader
/// module: the terrain's depth-only pipeline over the frame layout alone, back faces culled, with
/// the configuration's depth test and bias. The shader is the live shadow depth source, which
/// the move to the program runtime leaves as it was.
pub(crate) fn build_terrain_depth(
    device: &Device,
    frame_layout: &BindGroupLayout,
    configuration: worldrunner_render_shadow::DirectionalShadowConfig,
) -> RenderPipeline {
    let module = scene_shader(device, "shadow caster", &crate::shadows::depth_source());
    let vertices = vertex_attributes();
    let instances = instance_attributes();
    let buffers = vertex_buffers(&vertices, &instances);
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("terrain shadow caster"),
        bind_group_layouts: &[Some(frame_layout)],
        immediate_size: 0,
    });
    worldrunner_render_shadow::create_depth_pipeline(
        device,
        worldrunner_render_shadow::DepthPipelineDescriptor {
            label: Some("terrain"),
            layout: &layout,
            vertex: VertexState {
                module: &module,
                entry_point: Some("vs_terrain_depth"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: PrimitiveState {
                cull_mode: Some(Face::Back),
                ..Default::default()
            },
            compare: configuration.caster_compare,
            bias: configuration.caster_bias,
            fragment: None,
        },
    )
}

/// `draw_terrain_depth` of the fixed path for a cascade whose terrain selection is `keep`: the
/// pipeline, the cascade's frame group at set zero, then the geometry, a streamed chunk only when
/// `keep` accepts it.
pub(crate) fn draw_terrain_depth<C: Commands>(
    pass: &mut C,
    (pipeline, frame): (&RenderPipeline, &BindGroup),
    (geometry, instances): (&Geometry<'_>, &Buffer),
    keep: impl Fn(&Chunk) -> bool,
) -> DrawCount {
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, frame);
    let Geometry::Streaming(chunks) = geometry else {
        return super::draw::draw_terrain_geometry(pass, geometry, instances);
    };
    let mut count = DrawCount::default();
    for chunk in chunks.iter().filter(|chunk| keep(chunk)) {
        let offset = u64::from(chunk.record) * INSTANCE_STRIDE;
        pass.set_vertex_buffer(1, instances, Some(offset..offset + INSTANCE_STRIDE));
        pass.set_vertex_buffer(0, &chunk.vertices, None);
        pass.set_index_buffer(&chunk.indices, IndexFormat::Uint32);
        pass.draw_indexed(0..chunk.index_count, 0, 0..1);
        count.draw(1);
    }
    count
}
