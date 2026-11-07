//! The scene colour the stages after the reconstruction read.
//!
//! Native paths hand one texture to those stages. Temporal reconstruction writes only the history
//! texture of its parity, so its readers pick one of the two histories at encode time with the
//! same [`HistoryParity`] that picks the one the upscale writes. A reader is encoded in the same
//! command buffer as that writer, after it, and never in the pass that writes it.
use crate::{Stage, StageTarget, chain::Variant, history_parity::HistoryParity};

/// One texture, or the two temporal histories of which the parity selects the one written this frame.
#[derive(Clone, Debug)]
pub(crate) struct SceneColor {
    views: [wgpu::TextureView; 2],
    parity: Option<HistoryParity>,
}

impl SceneColor {
    pub fn single(view: wgpu::TextureView) -> Self {
        Self {
            views: [view.clone(), view],
            parity: None,
        }
    }

    /// `views[odd]` is the history the frame of parity `odd` writes.
    pub fn alternating(views: [wgpu::TextureView; 2], parity: HistoryParity) -> Self {
        Self {
            views,
            parity: Some(parity),
        }
    }

    pub fn view(&self, variant: usize) -> &wgpu::TextureView {
        &self.views[variant]
    }
}

/// What a pass binds as its source or lower input.
#[derive(Clone, Copy)]
pub(crate) enum Read<'a> {
    View(&'a wgpu::TextureView),
    Scene(&'a SceneColor),
}

impl<'a> From<&'a wgpu::TextureView> for Read<'a> {
    fn from(view: &'a wgpu::TextureView) -> Self {
        Self::View(view)
    }
}

impl<'a> Read<'a> {
    /// The texture bound in `variant`.
    pub fn view(self, variant: usize) -> &'a wgpu::TextureView {
        match self {
            Self::View(view) => view,
            Self::Scene(scene) => scene.view(variant),
        }
    }

    fn parity(self) -> Option<&'a HistoryParity> {
        match self {
            Self::View(_) => None,
            Self::Scene(scene) => scene.parity.as_ref(),
        }
    }
}

/// How many bind groups a pass that binds `reads` needs.
pub(crate) fn variants(reads: &[Read<'_>]) -> usize {
    1 + usize::from(reads.iter().any(|read| read.parity().is_some()))
}

fn parity<'a>(reads: &[Read<'a>]) -> Option<&'a HistoryParity> {
    reads.iter().find_map(|read| read.parity())
}

/// A fullscreen pass reading `reads`, with the bind group of each variant in `groups` (one per
/// [`variants`]). It alternates with the parity when a read does.
pub(crate) fn fullscreen_stage(
    id: crate::StageId,
    pipeline: &wgpu::RenderPipeline,
    (reads, groups): (&[Read<'_>], &[wgpu::BindGroup]),
    target: StageTarget,
) -> Stage {
    match (parity(reads), groups) {
        (Some(parity), [even, odd]) => Stage::fullscreen_alternating(
            id,
            pipeline,
            parity,
            [
                Variant {
                    groups: &[even],
                    target: target.clone(),
                },
                Variant {
                    groups: &[odd],
                    target,
                },
            ],
        ),
        (_, [only]) => Stage::fullscreen(id, pipeline, &[only], target),
        _ => unreachable!("one bind group per variant"),
    }
}

/// A compute pass reading `reads`, as [`fullscreen_stage`].
pub(crate) fn compute_stage(
    id: crate::StageId,
    pipeline: &wgpu::ComputePipeline,
    (reads, groups): (&[Read<'_>], &[wgpu::BindGroup]),
    dispatch: [u32; 3],
) -> Stage {
    match (parity(reads), groups) {
        (Some(parity), [even, odd]) => {
            Stage::compute_alternating(id, pipeline, parity, [even, odd], dispatch)
        }
        (_, [only]) => Stage {
            id,
            label: id.diagnostic_label(),
            enabled: true,
            operation: crate::Operation::Compute {
                pipeline: pipeline.clone(),
                groups: vec![only.clone()],
                dispatch,
            },
        },
        _ => unreachable!("one bind group per variant"),
    }
}
