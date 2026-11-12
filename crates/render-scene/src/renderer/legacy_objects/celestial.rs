//! The fixed celestial mesh path as it was at the freeze: `renderer::celestial_specialization`
//! with `build_variant`, and the draw loop of `sky::meshes::Meshes::draw_geometry`.
//!
//! The composed text (`renderer::celestial_source`), the sky lighting override values
//! (`sky::lighting::constants`), the vertex attributes and the key semantics are read live,
//! as the object freeze reads them: the move to the program runtime keeps all four, so the
//! captures compare how pipelines are built and draws recorded, from the same inputs.

use wgpu::{
    ColorTargetState, ColorWrites, Device, FragmentState, FrontFace, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor,
    ShaderSource, TextureFormat, VertexState,
};
use worldrunner_render_plan::SceneCelestialLighting;

use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::renderer::depth::DepthMode;
use crate::sky::meshes::MeshDraw;

/// What one selection was built from: the sky composition flags `(physical, separate,
/// object_exposure)`, the specular options and winding, the body's lighting and whether it is
/// anchored.
#[derive(Clone, Copy)]
pub(crate) struct Selection {
    pub(crate) composition: (bool, bool, bool),
    pub(crate) raster: ([u32; 7], FrontFace),
    pub(crate) lighting: SceneCelestialLighting,
    pub(crate) anchored: bool,
}

/// The 32 fixed pipelines of `selection`, built as `celestial_specialization` built them.
pub(crate) fn pipelines(
    device: &Device,
    scene: &ScenePipelines,
    selection: Selection,
) -> [RenderPipeline; PipelineKey::ALL.len()] {
    let (options, front_face) = selection.raster;
    let mut constants = crate::sky::lighting::constants(options, selection.lighting);
    if front_face == FrontFace::Cw {
        constants.push(("SCENE_RASTER_MIRRORED", 1.0));
    }
    let source = crate::renderer::celestial_source(
        selection.composition,
        selection.lighting,
        selection.anchored,
    );
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("celestial mesh lighting"),
        source: ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("celestial meshes"),
        bind_group_layouts: &[Some(scene.frame_layout()), Some(scene.material_layout())],
        immediate_size: 0,
    });
    let vertices = crate::geometry::vertex_attributes();
    let instances = crate::geometry::instance_attributes();
    let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba16Float,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let samples = scene.samples;
    PipelineKey::ALL.map(|key| {
        let mut constants = constants.clone();
        constants.extend(key.fragment_constants(samples));
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("environment"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &module,
                entry_point: Some("vs_celestial"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &buffers,
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face,
                cull_mode: key.cull_mode(),
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthMode::Independent.state(true)),
            multisample: MultisampleState {
                count: samples,
                alpha_to_coverage_enabled: key.alpha_to_coverage(samples),
                ..Default::default()
            },
            fragment: Some(FragmentState {
                module: &module,
                entry_point: Some(if key.masked {
                    "fs_celestial_masked"
                } else {
                    "fs_celestial_opaque"
                }),
                compilation_options: PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    })
}

/// Records `draws` as `draw_geometry` recorded them, each with the pipeline of its key from
/// the selection `select` builds for its lighting and anchoring. Returns how many it drew.
pub(crate) fn record(
    pass: &mut wgpu::RenderPass<'_>,
    frame: &wgpu::BindGroup,
    draws: &[MeshDraw],
    mut select: impl FnMut(SceneCelestialLighting, bool) -> [RenderPipeline; PipelineKey::ALL.len()],
) -> usize {
    for draw in draws {
        let pipelines = select(draw.lighting, draw.anchored);
        pass.set_pipeline(&pipelines[draw.key.index()]);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &draw.material, &[]);
        pass.set_vertex_buffer(0, draw.vertices.slice(..));
        pass.set_vertex_buffer(1, draw.instance.slice(..));
        pass.set_index_buffer(draw.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..draw.count, 0, 0..1);
    }
    draws.len()
}
