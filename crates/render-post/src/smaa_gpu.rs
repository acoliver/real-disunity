//! Schedules the three SMAA 1x passes. The shader is `smaa.wgsl`.
use wgpu::util::DeviceExt;

use crate::{SmaaSettings, Stage, StageId, StageSet, StageTarget, color_texture, smaa_lut};

/// Stage names, in execution order. Identities are stable for timing.
pub(crate) const STAGES: [&str; 3] = ["smaa_edges", "smaa_weights", "smaa_blend"];

struct Builder<'a> {
    device: &'a wgpu::Device,
    module: wgpu::ShaderModule,
    constants: [(&'static str, f64); 7],
}

struct Pass<'a> {
    entry: &'static str,
    entries: &'a [wgpu::BindGroupEntry<'a>],
    format: wgpu::TextureFormat,
    target: StageTarget,
}

impl Builder<'_> {
    fn stage(&self, pass: &Pass<'_>) -> Stage {
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(pass.entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(pass.entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &self.constants,
                        ..Default::default()
                    },
                    targets: &[Some(pass.format.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(pass.entry),
            layout: &pipeline.get_bind_group_layout(0),
            entries: pass.entries,
        });
        Stage::fullscreen(
            StageId::new(pass.entry, 0),
            &pipeline,
            &[&group],
            pass.target.clone(),
        )
    }
}

fn lookup_buffer(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE,
    })
}

fn texture_view(
    device: &wgpu::Device,
    set: &mut StageSet,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let texture = color_texture(device, label, size, format);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    set.resources.textures.push(texture);
    view
}

/// Intermediate targets and lookup data owned by one schedule.
struct Inputs {
    edges: wgpu::TextureView,
    weights: wgpu::TextureView,
    area: wgpu::Buffer,
    search: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

fn allocate(
    device: &wgpu::Device,
    set: &mut StageSet,
    size: [u32; 2],
    smaa: SmaaSettings,
) -> Inputs {
    Inputs {
        edges: texture_view(
            device,
            set,
            "SMAA edges",
            size,
            wgpu::TextureFormat::Rg8Unorm,
        ),
        weights: texture_view(
            device,
            set,
            "SMAA blending weights",
            size,
            wgpu::TextureFormat::Rgba8Unorm,
        ),
        area: lookup_buffer(
            device,
            "SMAA area lookup",
            &smaa_lut::area_texture(smaa.max_search_steps_diag.is_some()),
        ),
        search: lookup_buffer(device, "SMAA search lookup", &smaa_lut::search_texture()),
        sampler: device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SMAA edge sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }),
    }
}

fn view_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

/// Appends edge detection, weight calculation and neighborhood blending.
///
/// `source` is display-referred color at `size`. The edges (`Rg8Unorm`) and weights
/// (`Rgba8Unorm`) targets and the lookup buffers belong to `set`, so a resize or
/// reconfiguration rebuilds them with the rest of the schedule.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    size: [u32; 2],
    smaa: SmaaSettings,
    output: (wgpu::TextureFormat, StageTarget),
) {
    let builder = Builder {
        device,
        module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SMAA 1x"),
            source: wgpu::ShaderSource::Wgsl(include_str!("smaa.wgsl").into()),
        }),
        constants: smaa.constants(),
    };
    let inputs = allocate(device, set, size, smaa);
    let [edge_stage, weight_stage, blend_stage] = STAGES;
    set.stages.push(builder.stage(&Pass {
        entry: edge_stage,
        entries: &[view_entry(0, source)],
        format: wgpu::TextureFormat::Rg8Unorm,
        target: StageTarget::Texture(inputs.edges.clone()),
    }));
    set.stages.push(builder.stage(&Pass {
        entry: weight_stage,
        entries: &[
            view_entry(1, &inputs.edges),
            wgpu::BindGroupEntry {
                binding: 2,
                resource: inputs.area.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: inputs.search.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&inputs.sampler),
            },
        ],
        format: wgpu::TextureFormat::Rgba8Unorm,
        target: StageTarget::Texture(inputs.weights.clone()),
    }));
    set.stages.push(builder.stage(&Pass {
        entry: blend_stage,
        entries: &[view_entry(0, source), view_entry(4, &inputs.weights)],
        format: output.0,
        target: output.1,
    }));
    set.resources.buffers.extend([inputs.area, inputs.search]);
}
