//! Schedules the TAA resolve and history passes. The shader is `taa.wgsl`.
use crate::{Stage, StageId, StageSet, StageTarget, TaaSettings};

/// Stage names, in execution order. Identities are stable for timing.
pub const STAGES: [&str; 1] = ["taa resolve"];

/// Format of the resolve output, which is also the history of the next frame.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The two textures a resolve needs besides its inputs.
#[cfg(test)]
pub struct Targets {
    pub output: wgpu::Texture,
    pub history: wgpu::Texture,
}

/// Allocates the resolve output and a history texture, both `Rgba16Float` at `size`, with the
/// extra usages the tests need: the history is written by the test and both are read back.
#[cfg(test)]
pub fn allocate(device: &wgpu::Device, size: [u32; 2]) -> Targets {
    let readback = wgpu::TextureUsages::COPY_SRC;
    Targets {
        output: texture(device, "taa output", size, readback),
        history: texture(
            device,
            "taa history",
            size,
            readback | wgpu::TextureUsages::COPY_DST,
        ),
    }
}

/// A resolve output texture "taa output". It is rendered to, then sampled as the history of
/// the next frame's resolve. Each frame slot owns one.
pub fn output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(device, "taa output", size, wgpu::TextureUsages::empty())
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    extra: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | extra,
        view_formats: &[],
    })
}

/// Views and buffers one resolve reads and writes.
pub struct Inputs<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    /// Sample count of `depth`: 1 for a `texture_depth_2d`, more for a multisampled one.
    pub depth_samples: u32,
    pub uniform: &'a wgpu::Buffer,
    pub history: &'a wgpu::TextureView,
    pub output: &'a wgpu::TextureView,
}

/// Appends "taa resolve", which reads `inputs` and writes `inputs.output`. The caller binds
/// the previous frame's output as `inputs.history`; no copy pass follows.
///
/// `color` is scene-referred color a `textureLoad` reads, `depth` the single-sample device
/// depth of the same frame. Both are at the extent of `output` and `history`, which are
/// [`FORMAT`] textures. The uniform block is a [`crate::TaaUniform`].
/// A `depth` with `depth_samples` above 1 is a multisampled depth texture; the resolve then
/// takes the smallest depth over the samples of each texel. That case has its own shader
/// module, pipeline and layout.
pub fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    inputs: &Inputs<'_>,
    settings: TaaSettings,
) {
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
}

/// The shader source. The multisampled build differs only in the type of the `depth` binding.
fn shader(multisampled: bool) -> String {
    let source = include_str!("taa.wgsl");
    if multisampled {
        source.replace(DEPTH_BINDING, MULTISAMPLED_DEPTH_BINDING)
    } else {
        source.to_owned()
    }
}

const DEPTH_BINDING: &str = "var depth: texture_depth_2d;";
const MULTISAMPLED_DEPTH_BINDING: &str = "var depth: texture_depth_multisampled_2d;";

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
