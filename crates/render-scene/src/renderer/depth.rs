//! Depth ownership of a complete, single-sample stock geometry pair.
use wgpu::{
    CompareFunction, DepthStencilState, StencilFaceState, StencilOperation, StencilState,
    TextureFormat,
};

/// The stencil reference of the passes that own stencil ownership marks.
pub(crate) const STENCIL_REFERENCE: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DepthMode {
    #[default]
    Independent,
    /// The same depth representation without the prepass stencil (excluded forward draws).
    Shared,
    Prepass,
    Reuse,
}

impl DepthMode {
    pub(crate) fn vertex_source(self, source: &str) -> std::borrow::Cow<'_, str> {
        // Paired displacement producers and consumers retain one raster-position contract.
        if self == Self::Independent
            && !source.contains("fn scene_receiver_load(")
            && !source.contains("fn scene_capture_receiver(")
        {
            std::borrow::Cow::Owned(
                source.replace("@invariant @builtin(position)", "@builtin(position)"),
            )
        } else {
            std::borrow::Cow::Borrowed(source)
        }
    }

    pub(crate) const fn format(self) -> TextureFormat {
        match self {
            Self::Independent => TextureFormat::Depth32Float,
            Self::Shared | Self::Prepass | Self::Reuse => TextureFormat::Depth32FloatStencil8,
        }
    }

    /// Every mode resolves an exact depth tie to the last surface in API order: writing
    /// passes use `LessEqual`, and forward reuse tests the prepass depth with `Equal`.
    /// Reuse therefore only reads the prepass stencil mark, which excludes clear depth.
    /// Without a depth or stencil write, a fragment that discards can still be rejected
    /// before it is shaded, so hidden masked fragments cost no shading.
    pub(crate) fn state(self, write: bool) -> DepthStencilState {
        let face = StencilFaceState {
            compare: if self == Self::Reuse {
                CompareFunction::Equal
            } else {
                CompareFunction::Always
            },
            fail_op: StencilOperation::Keep,
            depth_fail_op: StencilOperation::Keep,
            pass_op: match self {
                Self::Prepass => StencilOperation::Replace,
                _ => StencilOperation::Keep,
            },
        };
        DepthStencilState {
            format: self.format(),
            depth_write_enabled: Some(write && self != Self::Reuse),
            depth_compare: Some(if self == Self::Reuse {
                CompareFunction::Equal
            } else {
                CompareFunction::LessEqual
            }),
            stencil: if matches!(self, Self::Prepass | Self::Reuse) {
                StencilState {
                    front: face,
                    back: face,
                    read_mask: 1,
                    write_mask: u32::from(self == Self::Prepass),
                }
            } else {
                StencilState::default()
            },
            bias: wgpu::DepthBiasState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepass_marks_depth_winners_and_forward_reads_the_mark_without_writing() {
        let seed = DepthMode::Prepass.state(true);
        assert_eq!(seed.depth_compare, Some(CompareFunction::LessEqual));
        assert_eq!(seed.depth_write_enabled, Some(true));
        assert_eq!(seed.stencil.front.pass_op, StencilOperation::Replace);
        assert_eq!(seed.stencil.front.depth_fail_op, StencilOperation::Keep);
        let forward = DepthMode::Reuse.state(true);
        assert_eq!(forward.depth_compare, Some(CompareFunction::Equal));
        assert_eq!(forward.depth_write_enabled, Some(false));
        assert_eq!(forward.stencil.front.compare, CompareFunction::Equal);
        assert_eq!(forward.stencil.front.pass_op, StencilOperation::Keep);
        assert_eq!(forward.stencil.front.fail_op, StencilOperation::Keep);
        assert_eq!(forward.stencil.front.depth_fail_op, StencilOperation::Keep);
        assert_eq!(forward.stencil.front, forward.stencil.back);
        assert_eq!(forward.stencil.read_mask, 1);
        assert_eq!(forward.stencil.write_mask, 0);
        assert_eq!(seed.stencil.write_mask, 1);
    }

    #[test]
    fn excluded_states_retain_depth_compare_writes_and_no_stencil_effect() {
        for mode in [DepthMode::Independent, DepthMode::Shared] {
            assert_eq!(
                mode.state(true).depth_compare,
                Some(CompareFunction::LessEqual)
            );
            assert_eq!(mode.state(true).depth_write_enabled, Some(true));
            assert_eq!(
                mode.state(false).depth_compare,
                Some(CompareFunction::LessEqual)
            );
            assert_eq!(mode.state(false).depth_write_enabled, Some(false));
            assert_eq!(mode.state(true).stencil, StencilState::default());
        }
    }

    #[test]
    fn stored_receiver_pairs_keep_invariant_positions_on_independent_depth() {
        use crate::scene_metadata::{
            MetadataOutput,
            receiver::{ReceiverLayout, consumer_source, producer_sources},
        };
        for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
            for output in [MetadataOutput::Normal, MetadataOutput::Merged] {
                let (object, terrain) = producer_sources(output, layout, 0.012);
                for source in [&object, &terrain] {
                    assert_eq!(
                        DepthMode::Independent.vertex_source(source).as_ref(),
                        source.as_str()
                    );
                }
            }
        }
        for reflection in [false, true] {
            for velocity in [false, true] {
                let (object, terrain) =
                    crate::shadows::receiver_sources(true, reflection, velocity);
                for (source, terrain) in [(&object, false), (&terrain, true)] {
                    let paired = consumer_source(source, ReceiverLayout::Front, terrain);
                    assert!(paired.contains("@invariant @builtin(position)"));
                    assert_eq!(
                        DepthMode::Independent.vertex_source(&paired).as_ref(),
                        paired.as_str()
                    );
                    assert!(
                        !DepthMode::Independent
                            .vertex_source(source)
                            .contains("@invariant @builtin(position)")
                    );
                }
            }
        }
    }
}
