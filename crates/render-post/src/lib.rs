//! Configured linear post/output chains with canonical targets, history ownership and scheduling.
//! Shader policy, adaptation rules and LUT data remain caller-authored.
pub mod allocation;
#[cfg(test)]
mod capture_tests;
pub mod contracts;
#[cfg(test)]
mod contracts_tests;
#[cfg(test)]
mod filter_hand_computed_gpu_tests;
#[cfg(test)]
mod filter_reference;
#[cfg(test)]
mod filter_reference_gpu_tests;
#[cfg(test)]
mod operations_buffer_scope_tests;
#[cfg(test)]
mod operations_compute_tests;
#[cfg(test)]
mod operations_copy_rules_tests;
#[cfg(test)]
mod operations_draw_tests;
#[cfg(test)]
mod operations_padding_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod operations_transfer_tests;
#[cfg(test)]
mod raster_capture_tests;
mod timing;
pub use timing::{
    FrameTiming, PassId, PassTimer, PassTiming, StageId, TIMING_RING_SIZE, TimingError, TimingMode,
    TimingSupport,
};
mod chain;
mod copy;
mod hdr_post;
mod post_filters;
mod tone_map;
mod upscale;
pub use hdr_post::{
    AdaptiveExposure, Bloom, HdrPostSettings, adapt_exposure, histogram_bin, target_exposure,
};
pub use tone_map::{ToneMap, ToneMapOutput};
pub use upscale::{RenderScale, SpatialUpscale, UpscaleMode, UpscaleSettings};
mod frame;
#[cfg(test)]
mod frame_tests;
pub use frame::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, BorrowedFeature, BuildContext,
    DrawContext, FeatureDeclaration, FeatureId, FeaturePass, FrameError, FrameFeature, FramePlan,
    PassDeclaration, PassSlot, PrepareContext, PreparedFeature, ResourceRef, validate_frame,
};
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
    TimingState,
};
pub use copy::CopyOutput;
pub use readback::{BufferReadback, Readback, ReadbackError, unpad_rows, wait_for_map};
pub use targets::{FrameConfig, FrameTargets, color_texture};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PostError {
    #[error("GPU allocation failed: {0}")]
    Allocation(String),
    #[error(transparent)]
    DriverAllocation(#[from] worldrunner_render_gpu::configuration::DriverAllocationError),
    #[error("unsupported frame profile: {0}")]
    UnsupportedProfile(&'static str),
    #[error("frame delta must be finite and within 0..=10 seconds")]
    FrameDelta,
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("post target extent is empty")]
    EmptyExtent,
    #[error("output extent must contain the render extent")]
    RenderExtent,
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
    fullscreen_pass_timed(encoder, label, attachment, pipeline, groups, None);
}

fn fullscreen_pass_timed(
    encoder: &mut wgpu::CommandEncoder,
    label: Option<&str>,
    attachment: wgpu::RenderPassColorAttachment<'_>,
    pipeline: &wgpu::RenderPipeline,
    groups: &[&wgpu::BindGroup],
    stamp: Option<&timing::Stamp>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label,
        timestamp_writes: stamp.map(timing::Stamp::render),
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

#[cfg(test)]
mod tone_map_gpu_tests;

#[cfg(test)]
fn test_authority(queue: &wgpu::Queue) -> worldrunner_render_gpu::submission::SubmissionAuthority {
    worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
}

#[cfg(test)]
mod upscale_tests;

#[cfg(test)]
mod upscale_hdr_tests;
