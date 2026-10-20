//! Shared GPU mechanics, not shadow policy.
//!
//! Callers own shader sources, vertex layouts, light fitting, cascade selection,
//! uniform encoding and cascade policy. This crate owns bounded square
//! `Depth32Float` maps, explicit receiver dimensions/bindings, depth-only pipelines
//! and far-depth clear/store passes, validated directional configuration and shared sampling kernels. No scene, asset or filesystem inputs are used.

mod config;
pub use config::{DirectionalCoverage, DirectionalShadowConfig, InvalidShadowConfig, ShadowFilter};
mod fit;
mod shader;
pub use fit::{DirectionalFit, InvalidFit};
pub const WGSL: &str = crate::shadow_wgsl!();

mod map;
mod pipeline;
mod receiver;

pub use map::{DepthMap, DepthMapBudget, DepthMapDescriptor, DepthMapError, DepthReadback};
pub use pipeline::{DepthFragmentState, DepthPipelineDescriptor, create_depth_pipeline};
pub use receiver::{ReceiverDescriptor, ReceiverDimension, comparison_sampler, receiver_layout};

/// The format shared by map allocation and depth-only pipelines.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Starts a depth-only pass, always clearing to far depth and storing the result.
/// Even a caller that skips all draws must run this pass to discard stale occluders.
pub fn begin_depth_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: Option<&str>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label,
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests;
