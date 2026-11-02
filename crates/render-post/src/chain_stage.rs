//! Stage targets, operations and their encoding.
use crate::{fullscreen_pass_timed, history_parity::HistoryParity};

/// Output is supplied per encode, allowing the same chain to target a surface or capture.
#[derive(Clone, Debug)]
pub enum StageTarget {
    Output,
    Texture(wgpu::TextureView),
    /// Two colour attachments, locations 0 and 1, both cleared and stored.
    Pair(wgpu::TextureView, wgpu::TextureView),
    /// Three colour attachments, locations 0, 1 and 2, all cleared and stored.
    Triple(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView),
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
    /// A compute dispatch of one pipeline with two bind groups, picked by `parity` like
    /// [`Self::FullscreenAlternating`].
    ComputeAlternating {
        pipeline: wgpu::ComputePipeline,
        parity: HistoryParity,
        groups: Box<[wgpu::BindGroup; 2]>,
        dispatch: [u32; 3],
    },
    /// A fullscreen pass of one pipeline with two sets of bind groups and targets. `parity`
    /// picks the set at encode time: the first when even, the second when odd.
    FullscreenAlternating {
        pipeline: wgpu::RenderPipeline,
        parity: HistoryParity,
        variants: Box<[(Vec<wgpu::BindGroup>, StageTarget); 2]>,
    },
}

/// One bind group set and target of an alternating fullscreen pass.
pub struct Variant<'a> {
    pub groups: &'a [&'a wgpu::BindGroup],
    pub target: StageTarget,
}

impl Operation {
    /// Whether the operation is a compute or render pass, which has an identity and a timing slot.
    pub(super) fn is_pass(&self) -> bool {
        !matches!(
            self,
            Self::Clear(_) | Self::CopyBuffer { .. } | Self::CopyTexture { .. }
        )
    }

    /// Whether the operation is a fullscreen pass that writes the chain's output in every variant.
    pub(super) fn writes_output(&self) -> bool {
        match self {
            Self::Fullscreen { target, .. } => matches!(target, StageTarget::Output),
            Self::FullscreenAlternating { variants, .. } => variants
                .iter()
                .all(|(_, target)| matches!(target, StageTarget::Output)),
            _ => false,
        }
    }

    /// The workgroup counts of a compute pass whose counts the stage itself fixes.
    pub(super) fn direct_dispatch(&self) -> Option<[u32; 3]> {
        match self {
            Self::Compute { dispatch, .. } | Self::ComputeAlternating { dispatch, .. } => {
                Some(*dispatch)
            }
            _ => None,
        }
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

    /// A fullscreen pass whose bind groups and target alternate with the parity of the history.
    pub(crate) fn fullscreen_alternating(
        id: crate::StageId,
        pipeline: &wgpu::RenderPipeline,
        parity: &HistoryParity,
        variants: [Variant<'_>; 2],
    ) -> Self {
        let owned = variants.map(|variant| {
            let groups = variant.groups.iter().map(|group| (*group).clone());
            (groups.collect(), variant.target)
        });
        Self {
            id,
            label: id.name,
            enabled: true,
            operation: Operation::FullscreenAlternating {
                pipeline: pipeline.clone(),
                parity: parity.clone(),
                variants: Box::new(owned),
            },
        }
    }

    /// A compute dispatch whose bind group alternates with the parity of the history.
    pub(crate) fn compute_alternating(
        id: crate::StageId,
        pipeline: &wgpu::ComputePipeline,
        parity: &HistoryParity,
        groups: [&wgpu::BindGroup; 2],
        dispatch: [u32; 3],
    ) -> Self {
        Self {
            id,
            label: id.name,
            enabled: true,
            operation: Operation::ComputeAlternating {
                pipeline: pipeline.clone(),
                parity: parity.clone(),
                groups: Box::new(groups.map(wgpu::BindGroup::clone)),
                dispatch,
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

    fn encode_compute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        stamp: Option<&crate::timing::Stamp>,
    ) {
        match &self.operation {
            Operation::Compute {
                pipeline,
                groups,
                dispatch,
            } => {
                let mut pass = self.compute_pass(encoder, stamp, pipeline, groups);
                pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
            }
            Operation::ComputeAlternating {
                pipeline,
                parity,
                groups,
                dispatch,
            } => {
                let group = &groups[usize::from(parity.odd())];
                let mut pass =
                    self.compute_pass(encoder, stamp, pipeline, std::slice::from_ref(group));
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
            _ => unreachable!("only compute operations are encoded here"),
        }
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
            Operation::Compute { .. }
            | Operation::ComputeAlternating { .. }
            | Operation::ComputeIndirect { .. } => self.encode_compute(encoder, stamp),
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
            Operation::FullscreenAlternating {
                pipeline,
                parity,
                variants,
            } => {
                let (groups, target) = &variants[usize::from(parity.odd())];
                self.fullscreen_pass(
                    encoder,
                    stamp,
                    (pipeline, groups),
                    &views_of(target, output),
                );
            }
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
        StageTarget::Triple(first, second, third) => vec![first, second, third],
    }
}
