//! Schedules the TAA resolve and history passes. The shader is `taa.wgsl`.
//!
//! Exported until `HdrPost` schedules these passes itself.
use crate::{Stage, StageId, StageSet, StageTarget, TaaSettings};

/// Stage names, in execution order. Identities are stable for timing.
pub const STAGES: [&str; 2] = ["taa resolve", "taa history"];

/// Format of the resolve output and of the history texture.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The two textures a resolve needs besides its inputs.
pub struct Targets {
    pub output: wgpu::Texture,
    pub history: wgpu::Texture,
}

/// Allocates the resolve output "taa output" and the history "taa history", both
/// `Rgba16Float` at `size`. The output is rendered to, sampled and copied from. The history is
/// also rendered to by the copy-back pass and may be written by the host (`COPY_DST`).
pub fn allocate(device: &wgpu::Device, size: [u32; 2]) -> Targets {
    let texture = |label, extra| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: crate::targets::extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | extra,
            view_formats: &[],
        })
    };
    Targets {
        output: texture("taa output", wgpu::TextureUsages::empty()),
        history: texture("taa history", wgpu::TextureUsages::COPY_DST),
    }
}

/// Views and buffers one resolve reads and writes.
pub struct Inputs<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub uniform: &'a wgpu::Buffer,
    pub history: &'a wgpu::TextureView,
    pub output: &'a wgpu::TextureView,
}

/// Appends "taa resolve", which reads `inputs` and writes `inputs.output`, then "taa history",
/// which copies the output into `inputs.history`. One history texture plus a copy-back keeps
/// the bind groups static, which stage sets that are built once per frame slot need.
///
/// `color` is scene-referred color a `textureLoad` reads, `depth` the single-sample device
/// depth of the same frame. Both are at the extent of `output` and `history`, which are
/// [`FORMAT`] textures made by [`allocate`]. The uniform block is a [`crate::TaaUniform`].
/// Multisampled depth is not supported yet.
pub fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    inputs: &Inputs<'_>,
    settings: TaaSettings,
) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TAA resolve"),
        source: wgpu::ShaderSource::Wgsl(include_str!("taa.wgsl").into()),
    });
    let constants = [
        ("HISTORY_WEIGHT", f64::from(settings.history_weight)),
        ("CLIP_SCALE", f64::from(settings.clip_scale)),
    ];
    let resolve = pipeline(device, &module, "taa_resolve", &constants);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("TAA history"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let resolve_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGES[0]),
        layout: &resolve.get_bind_group_layout(0),
        entries: &[
            entry(0, wgpu::BindingResource::TextureView(inputs.color)),
            entry(1, wgpu::BindingResource::TextureView(inputs.depth)),
            entry(2, wgpu::BindingResource::TextureView(inputs.history)),
            entry(3, wgpu::BindingResource::Sampler(&sampler)),
            entry(4, inputs.uniform.as_entire_binding()),
        ],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new(STAGES[0], 0),
        &resolve,
        &[&resolve_group],
        StageTarget::Texture(inputs.output.clone()),
    ));
    let copy = pipeline(device, &module, "taa_history", &[]);
    let copy_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGES[1]),
        layout: &copy.get_bind_group_layout(0),
        entries: &[entry(0, wgpu::BindingResource::TextureView(inputs.output))],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new(STAGES[1], 0),
        &copy,
        &[&copy_group],
        StageTarget::Texture(inputs.history.clone()),
    ));
}

fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

/// A fullscreen pipeline with a layout derived from the entry point's own bindings.
fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    fragment: &str,
    constants: &[(&str, f64)],
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            },
            targets: &[Some(FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
