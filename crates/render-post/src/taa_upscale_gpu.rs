//! Schedules the temporal reconstruction stage. The shader is `taa_upscale.wgsl` after the shared
//! `taa_common.wgsl` helpers, after the reconstruction stages of `taa_reconstruct_gpu`.
//!
//! It reads the jittered current color at the internal extent `R` and writes the output extent
//! `O`, then copies its result into the history. The uniform block is a [`crate::TemporalBlock`], the camera block, read by the reconstruction,
//! a [`crate::TemporalCamera`].
use crate::{Stage, StageId, StageSet, StageTarget, taa_reconstruct_gpu};

/// Stage names, in execution order. Identities are stable for timing.
pub const STAGES: [&str; 2] = ["temporal upscale", "temporal upscale history"];

/// Format of the output and of the history, and of the current color it reads.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The output texture "temporal upscale output" at the output extent. Frame slots each own one.
pub fn output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal upscale output",
        size,
        wgpu::TextureUsages::empty(),
    )
}

/// The history texture "temporal upscale history" at the output extent, shared by every frame
/// slot like the native history.
pub fn history(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    texture(
        device,
        "temporal upscale history",
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
}

/// Views and buffers one reconstruction reads and writes.
pub struct Inputs<'a> {
    /// Jittered scene color at `R`, [`FORMAT`].
    pub color: &'a wgpu::TextureView,
    /// Object residual of the backward motion in uv units at `R`, [`crate::VELOCITY_FORMAT`].
    pub motion: &'a wgpu::TextureView,
    /// Single-sample device depth at `R`, standard Z.
    pub depth: &'a wgpu::TextureView,
    /// The internal extent `R`.
    pub render: [u32; 2],
    pub uniform: &'a wgpu::Buffer,
    /// A [`crate::TemporalCamera`] block.
    pub camera: &'a wgpu::Buffer,
    /// [`history`] at `O`.
    pub history: &'a wgpu::Texture,
    /// [`output`] at `O`.
    pub output: &'a wgpu::Texture,
}

/// Appends the stages of [`taa_reconstruct_gpu`], "temporal upscale", which writes
/// `inputs.output` from the total motion they produce, then "temporal upscale history", a texture
/// copy of it into `inputs.history`. Returns the resources of the reconstruction stages, which the
/// caller keeps alive with the set.
pub fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    inputs: &Inputs<'_>,
) -> taa_reconstruct_gpu::Outputs {
    let reconstructed = taa_reconstruct_gpu::append(
        device,
        set,
        &taa_reconstruct_gpu::Inputs {
            depth: inputs.depth,
            motion: inputs.motion,
            camera: inputs.camera,
            size: inputs.render,
        },
    );
    let motion = reconstructed
        .motion
        .create_view(&wgpu::TextureViewDescriptor::default());
    let history = inputs
        .history
        .create_view(&wgpu::TextureViewDescriptor::default());
    let output = inputs
        .output
        .create_view(&wgpu::TextureViewDescriptor::default());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("temporal upscale"),
        source: wgpu::ShaderSource::Wgsl(
            concat!(
                include_str!("taa_common.wgsl"),
                include_str!("taa_upscale.wgsl")
            )
            .into(),
        ),
    });
    let pipeline = pipeline(device, &module);
    let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGES[0]),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            entry(0, wgpu::BindingResource::TextureView(inputs.color)),
            entry(1, wgpu::BindingResource::TextureView(&history)),
            entry(2, wgpu::BindingResource::TextureView(&motion)),
            entry(3, inputs.uniform.as_entire_binding()),
        ],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new(STAGES[0], 0),
        &pipeline,
        &[&group],
        StageTarget::Texture(output),
    ));
    set.stages.push(Stage::copy_texture(
        StageId::new(STAGES[1], 0),
        inputs.output,
        inputs.history,
    ));
    reconstructed
}

fn pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("taa_upscale"),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("taa_upscale"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
