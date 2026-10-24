//! Configured linear post/output chains with canonical targets, history ownership and scheduling.
//! Shader policy, adaptation rules and LUT data remain caller-authored.
mod chain;
mod copy;
#[cfg(test)]
mod copy_gpu_tests;
mod exact;
#[cfg(test)]
mod exact_gpu_tests;
#[cfg(test)]
mod exact_tests;
pub use exact::ExactSrgb8;
#[cfg(test)]
mod gpu_tests;
mod readback;
mod shader;
mod targets;
pub use chain::{
    BufferSpec, Operation, PostChain, Resources, Stage, StageFactory, StageSet, StageTarget,
};
pub use copy::CopyOutput;
pub use readback::{BufferReadback, Readback, ReadbackError, unpad_rows, wait_for_map};
pub use targets::{FrameConfig, FrameTargets, color_texture};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PostError {
    #[error("post target extent is empty")]
    EmptyExtent,
    #[error("post extent {size:?} exceeds texture limit {limit}")]
    TextureLimit { size: [u32; 2], limit: u32 },
    #[error("invalid post sample count {0}")]
    SampleCount(u32),
    #[error("unsupported post attachment format {0:?}")]
    UnsupportedFormat(wgpu::TextureFormat),
    #[error("post color attachment cannot use depth/stencil format {0:?}")]
    NonColorFormat(wgpu::TextureFormat),
    #[error("post attachment format {0:?} does not support multisample resolve")]
    UnsupportedResolve(wgpu::TextureFormat),
    #[error("post copy source requires a float texture, got {0:?}")]
    CopySourceFormat(wgpu::TextureFormat),
    #[error("post copy output requires a renderable float color format, got {0:?}")]
    CopyOutputFormat(wgpu::TextureFormat),
    #[error("exact linear composition requires enabled FLOAT32_BLENDABLE")]
    ExactCompositionFeatures,
    #[error("exact output requires single-sample Rgba32Float composition")]
    ExactCompositionFormat,
    #[error("invalid post resource buffer size {0}")]
    BufferSize(u64),
    #[error("post chain needs exactly one enabled final output stage")]
    OutputStage,
    #[error("post compute dispatch {0:?} exceeds device limits or is empty")]
    Dispatch([u32; 3]),
    #[error("post readback source does not match its target contract")]
    ReadbackSource,
}
#[cfg(test)]
mod chain_tests;
pub const WGSL: &str = crate::post_wgsl!();

/// Explicit stages and output contract for a single-sample fullscreen triangle.
#[derive(Clone, Copy, Debug)]
pub struct FullscreenDescriptor<'a> {
    pub label: Option<&'a str>,
    pub layout: &'a wgpu::PipelineLayout,
    pub module: &'a wgpu::ShaderModule,
    pub vertex_entry: &'a str,
    pub fragment_entry: &'a str,
    pub format: wgpu::TextureFormat,
    pub constants: &'a [(&'a str, f64)],
}

pub fn fullscreen_pipeline(
    device: &wgpu::Device,
    desc: FullscreenDescriptor<'_>,
) -> wgpu::RenderPipeline {
    let options = wgpu::PipelineCompilationOptions {
        constants: desc.constants,
        ..Default::default()
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: desc.label,
        layout: Some(desc.layout),
        vertex: wgpu::VertexState {
            module: desc.module,
            entry_point: Some(desc.vertex_entry),
            compilation_options: options.clone(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: desc.module,
            entry_point: Some(desc.fragment_entry),
            compilation_options: options,
            targets: &[Some(wgpu::ColorTargetState {
                format: desc.format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ComputeDescriptor<'a> {
    pub label: Option<&'a str>,
    pub layout: &'a wgpu::PipelineLayout,
    pub module: &'a wgpu::ShaderModule,
    pub entry: &'a str,
    pub constants: &'a [(&'a str, f64)],
}

pub fn compute_pipeline(
    device: &wgpu::Device,
    desc: ComputeDescriptor<'_>,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: desc.label,
        layout: Some(desc.layout),
        module: desc.module,
        entry_point: Some(desc.entry),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: desc.constants,
            ..Default::default()
        },
        cache: None,
    })
}

/// Load/store and resolve are authored by the caller; no implicit clear or submission.
pub fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: Option<&str>,
    attachment: wgpu::RenderPassColorAttachment<'_>,
    pipeline: &wgpu::RenderPipeline,
    groups: &[&wgpu::BindGroup],
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label,
        color_attachments: &[Some(attachment)],
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    for (index, group) in groups.iter().enumerate() {
        pass.set_bind_group(
            u32::try_from(index).expect("GPU bind group index fits u32"),
            *group,
            &[],
        );
    }
    pass.draw(0..3, 0..1);
}

/// Rational tone curve with authored coefficients; no engine-owned display look.
#[derive(Clone, Copy, Debug)]
pub struct RationalCurve {
    pub numerator: [f32; 3],
    pub denominator: [f32; 3],
}
impl RationalCurve {
    pub fn evaluate(self, value: f32) -> f32 {
        let [quadratic_num, linear_num, constant_num] = self.numerator;
        let [quadratic_den, linear_den, constant_den] = self.denominator;
        (value * (quadratic_num * value + linear_num) + constant_num)
            / (value * (quadratic_den * value + linear_den) + constant_den)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod readback_gpu_tests;
