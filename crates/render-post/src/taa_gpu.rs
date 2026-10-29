//! Schedules the TAA resolve and history passes. The shader is `taa.wgsl`.
use crate::{Stage, StageId, StageSet, StageTarget, TaaSettings};

/// Stage names, in execution order. Identities are stable for timing.
pub const STAGES: [&str; 3] = ["taa resolve", "taa history", "taa depth history"];

/// Format of the resolve output and of the history texture.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Format of the linear-depth output and of the depth history.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// The two textures a resolve needs besides its inputs.
#[cfg(test)]
pub struct Targets {
    pub output: wgpu::Texture,
    pub history: wgpu::Texture,
    pub depth_output: wgpu::Texture,
    pub depth_history: wgpu::Texture,
}

/// Allocates the resolve output "taa output" and the history "taa history", both
/// `Rgba16Float` at `size`. The output is rendered to, sampled and copied from. The history is
/// the destination of the copy-back (`COPY_DST`) and may also be written by the host.
#[cfg(test)]
pub fn allocate(device: &wgpu::Device, size: [u32; 2]) -> Targets {
    Targets {
        output: output(device, size),
        history: history(device, size),
        depth_output: depth_output(device, size),
        depth_history: depth_history(device, size),
    }
}

/// The resolve output texture "taa output". Frame slots each own one.
pub fn output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(device, "taa output", size, wgpu::TextureUsages::empty())
}

/// The history texture "taa history". One is shared by every frame slot of a size.
pub fn history(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(device, "taa history", size, wgpu::TextureUsages::COPY_DST)
}

/// The linear-depth output "taa depth output", the second target of the resolve. Frame slots
/// each own one.
pub fn depth_output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    format_texture(
        device,
        ("taa depth output", DEPTH_FORMAT),
        size,
        wgpu::TextureUsages::empty(),
    )
}

/// The depth history "taa depth history", the linear view depth of the previous frame's
/// closest-depth texels. One is shared by every frame slot of a size, like [`history`].
pub fn depth_history(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    format_texture(
        device,
        ("taa depth history", DEPTH_FORMAT),
        size,
        wgpu::TextureUsages::COPY_DST,
    )
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    extra: wgpu::TextureUsages,
) -> wgpu::Texture {
    format_texture(device, (label, FORMAT), size, extra)
}

fn format_texture(
    device: &wgpu::Device,
    (label, format): (&str, wgpu::TextureFormat),
    size: [u32; 2],
    extra: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | extra,
        view_formats: &[],
    })
}

/// Format of the object-motion residual texture a resolve reads.
pub const VELOCITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// Views and buffers one resolve reads and writes.
pub struct Inputs<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    /// Sample count of `depth`: 1 for a `texture_depth_2d`, more for a multisampled one.
    pub depth_samples: u32,
    /// Object-motion residual, `Rg16Float` (see `taa.wgsl`). `None` binds a zero texture, so
    /// the resolve uses camera reprojection alone.
    pub velocity: Option<&'a wgpu::TextureView>,
    /// Sample count of `velocity`; it must equal `depth_samples` (the winning depth sample
    /// selects the velocity sample). Ignored when `velocity` is `None`.
    pub velocity_samples: u32,
    pub uniform: &'a wgpu::Buffer,
    pub history: &'a wgpu::Texture,
    pub output: &'a wgpu::Texture,
    /// [`DEPTH_FORMAT`] linear depth of the previous frame, read by the disocclusion test.
    pub depth_history: &'a wgpu::Texture,
    /// [`DEPTH_FORMAT`] second target of the resolve, copied into `depth_history` afterwards.
    pub depth_output: &'a wgpu::Texture,
}

/// Appends "taa resolve", which reads `inputs` and writes `inputs.output` and
/// `inputs.depth_output`, then "taa history" and "taa depth history", texture copies of the two
/// outputs into `inputs.history` and `inputs.depth_history`. A depth texture cannot be copied
/// instead: the frame depth may be multisampled and has no `COPY_SRC` guarantee. One history texture plus a copy-back
/// keeps the bind groups static and independent of which frame slot runs, which stage sets that
/// are built once per frame slot need. A copy is cheaper than the fullscreen pass it replaces.
///
/// `color` is scene-referred color a `textureLoad` reads, `depth` the single-sample device
/// depth of the same frame. Both are at the extent of `output` and `history`, which are
/// [`FORMAT`] textures made by [`allocate`]. The uniform block is a [`crate::TaaUniform`].
/// A `depth` with `depth_samples` above 1 is a multisampled depth texture; the resolve then
/// takes the smallest depth over the samples of each texel. That case has its own shader
/// module, pipeline and layout.
pub fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    inputs: &Inputs<'_>,
    settings: TaaSettings,
) {
    let history = inputs
        .history
        .create_view(&wgpu::TextureViewDescriptor::default());
    let output = inputs
        .output
        .create_view(&wgpu::TextureViewDescriptor::default());
    let depth_output = inputs
        .depth_output
        .create_view(&wgpu::TextureViewDescriptor::default());
    let depth_history = inputs
        .depth_history
        .create_view(&wgpu::TextureViewDescriptor::default());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TAA resolve"),
        source: wgpu::ShaderSource::Wgsl(shader(inputs.depth_samples > 1).into()),
    });
    let constants = [
        ("HISTORY_WEIGHT", f64::from(settings.history_weight)),
        ("CLIP_SCALE", f64::from(settings.clip_scale)),
        ("SAMPLES", f64::from(inputs.depth_samples.max(1))),
    ];
    let resolve = pipeline(device, &module, "taa_resolve", &constants);
    let zero = zero_velocity(device, inputs.depth_samples.max(1));
    let velocity = inputs.velocity.map_or(zero, |view| {
        assert_eq!(
            inputs.velocity_samples,
            inputs.depth_samples.max(1),
            "velocity and depth must have the same sample count"
        );
        view.clone()
    });
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
            entry(2, wgpu::BindingResource::TextureView(&history)),
            entry(3, wgpu::BindingResource::Sampler(&sampler)),
            entry(4, inputs.uniform.as_entire_binding()),
            entry(5, wgpu::BindingResource::TextureView(&velocity)),
            entry(6, wgpu::BindingResource::TextureView(&depth_history)),
        ],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new(STAGES[0], 0),
        &resolve,
        &[&resolve_group],
        StageTarget::Pair(output, depth_output),
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[1], 0),
        inputs.output,
        inputs.history,
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[2], 0),
        inputs.depth_output,
        inputs.depth_history,
    ));
}

/// The residual a resolve reads when the host has none: one `Rg16Float` texel, zero from
/// creation, with `samples` samples so that it matches the module's `velocity` binding.
fn zero_velocity(device: &wgpu::Device, samples: u32) -> wgpu::TextureView {
    let extra = if samples > 1 {
        wgpu::TextureUsages::RENDER_ATTACHMENT
    } else {
        wgpu::TextureUsages::empty()
    };
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("taa zero velocity"),
            size: crate::targets::extent([1, 1]),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: VELOCITY_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | extra,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// The shader source. The multisampled build differs only in the types of the `depth` and
/// `velocity` bindings.
fn shader(multisampled: bool) -> String {
    let source = include_str!("taa.wgsl");
    if multisampled {
        source
            .replace(DEPTH_BINDING, MULTISAMPLED_DEPTH_BINDING)
            .replace(VELOCITY_BINDING, MULTISAMPLED_VELOCITY_BINDING)
    } else {
        source.to_owned()
    }
}

const DEPTH_BINDING: &str = "var depth: texture_depth_2d;";
const MULTISAMPLED_DEPTH_BINDING: &str = "var depth: texture_depth_multisampled_2d;";
const VELOCITY_BINDING: &str = "var velocity: texture_2d<f32>;";
const MULTISAMPLED_VELOCITY_BINDING: &str = "var velocity: texture_multisampled_2d<f32>;";

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
            targets: &[Some(FORMAT.into()), Some(DEPTH_FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
