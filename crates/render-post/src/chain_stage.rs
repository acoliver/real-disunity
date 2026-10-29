//! Stage targets, operations and their encoding.
use crate::fullscreen_pass_timed;

/// Output is supplied per encode, allowing the same chain to target a surface or capture.
#[derive(Clone, Debug)]
pub enum StageTarget {
    Output,
    Texture(wgpu::TextureView),
    /// Two colour attachments, locations 0 and 1, both cleared and stored.
    Pair(wgpu::TextureView, wgpu::TextureView),
}

/// A linear authored sequence, not a render graph. Bind groups carry explicit caller layouts.
#[derive(Debug)]
pub enum Operation {
    Clear(wgpu::Buffer),
    CopyBuffer {
        source: wgpu::Buffer,
        destination: wgpu::Buffer,
        bytes: u64,
    },
    /// Copies the whole of `source` into `destination`. Both are single-mip 2D textures of the
    /// same format and extent, with `COPY_SRC` and `COPY_DST` usage respectively.
    CopyTexture {
        source: wgpu::Texture,
        destination: wgpu::Texture,
    },
    Compute {
        pipeline: wgpu::ComputePipeline,
        groups: Vec<wgpu::BindGroup>,
        dispatch: [u32; 3],
    },
    /// A compute dispatch sized by three `u32` workgroup counts that an earlier stage wrote
    /// at `offset` of `arguments`, a buffer with `INDIRECT` usage.
    ComputeIndirect {
        pipeline: wgpu::ComputePipeline,
        groups: Vec<wgpu::BindGroup>,
        arguments: wgpu::Buffer,
        offset: u64,
    },
    Fullscreen {
        pipeline: wgpu::RenderPipeline,
        groups: Vec<wgpu::BindGroup>,
        target: StageTarget,
    },
}

impl Operation {
    /// Whether the operation is a compute or render pass, which has an identity and a timing slot.
    pub(super) fn is_pass(&self) -> bool {
        !matches!(
            self,
            Self::Clear(_) | Self::CopyBuffer { .. } | Self::CopyTexture { .. }
        )
    }
}

#[derive(Debug)]
pub struct Stage {
    pub id: crate::StageId,
    pub label: &'static str,
    pub enabled: bool,
    pub operation: Operation,
}

impl Stage {
    /// A texture-to-texture copy, encoded outside any pass and so without a timing slot.
    pub fn copy_texture(
        id: crate::StageId,
        source: &wgpu::Texture,
        destination: &wgpu::Texture,
    ) -> Self {
        Self {
            id,
            label: id.name,
            enabled: true,
            operation: Operation::CopyTexture {
                source: source.clone(),
                destination: destination.clone(),
            },
        }
    }

    pub fn fullscreen(
        id: crate::StageId,
        pipeline: &wgpu::RenderPipeline,
        groups: &[&wgpu::BindGroup],
        target: StageTarget,
    ) -> Self {
        Self {
            id,
            label: id.name,
            enabled: true,
            operation: Operation::Fullscreen {
                pipeline: pipeline.clone(),
                groups: groups.iter().map(|group| (*group).clone()).collect(),
                target,
            },
        }
    }

    fn compute_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        pipeline: &wgpu::ComputePipeline,
        groups: &[wgpu::BindGroup],
    ) -> wgpu::ComputePass<'e> {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(self.label),
            timestamp_writes: stamp.map(crate::timing::Stamp::compute),
        });
        pass.set_pipeline(pipeline);
        for (index, group) in groups.iter().enumerate() {
            pass.set_bind_group(
                u32::try_from(index).expect("validated GPU group index"),
                group,
                &[],
            );
        }
        pass
    }

    fn fullscreen_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
        (pipeline, groups): (&wgpu::RenderPipeline, &[wgpu::BindGroup]),
        views: &[&wgpu::TextureView],
    ) {
        let attachments: Vec<_> = views
            .iter()
            .map(|view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect();
        fullscreen_pass_timed(
            encoder,
            Some(self.label),
            &attachments,
            pipeline,
            &groups.iter().collect::<Vec<_>>(),
            stamp,
        );
    }

    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        stamp: Option<&crate::timing::Stamp>,
    ) {
        if !self.enabled {
            return;
        }
        match &self.operation {
            Operation::Clear(buffer) => encoder.clear_buffer(buffer, 0, None),
            Operation::CopyBuffer {
                source,
                destination,
                bytes,
            } => encoder.copy_buffer_to_buffer(source, 0, destination, 0, *bytes),
            Operation::CopyTexture {
                source,
                destination,
            } => encoder.copy_texture_to_texture(
                source.as_image_copy(),
                destination.as_image_copy(),
                source.size(),
            ),
            Operation::Compute {
                pipeline,
                groups,
                dispatch,
            } => {
                let mut pass = self.compute_pass(encoder, stamp, pipeline, groups);
                pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
            }
            Operation::ComputeIndirect {
                pipeline,
                groups,
                arguments,
                offset,
            } => {
                let mut pass = self.compute_pass(encoder, stamp, pipeline, groups);
                pass.dispatch_workgroups_indirect(arguments, *offset);
            }
            Operation::Fullscreen {
                pipeline,
                groups,
                target,
            } => self.fullscreen_pass(
                encoder,
                stamp,
                (pipeline, groups),
                &views_of(target, output),
            ),
        }
    }
}

fn views_of<'a>(
    target: &'a StageTarget,
    output: &'a wgpu::TextureView,
) -> Vec<&'a wgpu::TextureView> {
    match target {
        StageTarget::Output => vec![output],
        StageTarget::Texture(view) => vec![view],
        StageTarget::Pair(first, second) => vec![first, second],
    }
}
