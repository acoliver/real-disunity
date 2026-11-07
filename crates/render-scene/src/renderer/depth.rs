//! Depth ownership of a complete, single-sample stock geometry pair.
use wgpu::{
    CompareFunction, DepthStencilState, StencilFaceState, StencilOperation, StencilState,
    TextureFormat,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DepthMode {
    #[default]
    Independent,
    /// The same depth representation, without consuming ownership (excluded forward draws).
    Shared,
    Prepass,
    Reuse,
}

impl DepthMode {
    pub(crate) const fn format(self) -> TextureFormat {
        match self {
            Self::Independent => TextureFormat::Depth32Float,
            Self::Shared | Self::Prepass | Self::Reuse => TextureFormat::Depth32FloatStencil8,
        }
    }

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
                Self::Reuse => StencilOperation::Zero,
                _ => StencilOperation::Keep,
            },
        };
        DepthStencilState {
            format: self.format(),
            depth_write_enabled: Some(write && self != Self::Reuse),
            depth_compare: Some(if self == Self::Reuse {
                CompareFunction::Equal
            } else if write {
                CompareFunction::Less
            } else {
                CompareFunction::LessEqual
            }),
            stencil: if matches!(self, Self::Prepass | Self::Reuse) {
                StencilState {
                    front: face,
                    back: face,
                    read_mask: 1,
                    write_mask: 1,
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
    fn prepass_marks_only_depth_winners_and_forward_consumes_first_equal_fragment() {
        let seed = DepthMode::Prepass.state(true);
        assert_eq!(seed.depth_compare, Some(CompareFunction::Less));
        assert_eq!(seed.depth_write_enabled, Some(true));
        assert_eq!(seed.stencil.front.pass_op, StencilOperation::Replace);
        assert_eq!(seed.stencil.front.depth_fail_op, StencilOperation::Keep);
        let forward = DepthMode::Reuse.state(true);
        assert_eq!(forward.depth_compare, Some(CompareFunction::Equal));
        assert_eq!(forward.depth_write_enabled, Some(false));
        assert_eq!(forward.stencil.front.compare, CompareFunction::Equal);
        assert_eq!(forward.stencil.front.pass_op, StencilOperation::Zero);
        assert_eq!(forward.stencil.front.fail_op, StencilOperation::Keep);
        assert_eq!(forward.stencil.front.depth_fail_op, StencilOperation::Keep);
        assert_eq!(forward.stencil.front, forward.stencil.back);
        assert_eq!(forward.stencil.read_mask, 1);
        assert_eq!(forward.stencil.write_mask, 1);
    }

    #[test]
    fn excluded_states_retain_depth_compare_writes_and_no_stencil_effect() {
        for mode in [DepthMode::Independent, DepthMode::Shared] {
            assert_eq!(mode.state(true).depth_compare, Some(CompareFunction::Less));
            assert_eq!(mode.state(true).depth_write_enabled, Some(true));
            assert_eq!(
                mode.state(false).depth_compare,
                Some(CompareFunction::LessEqual)
            );
            assert_eq!(mode.state(false).depth_write_enabled, Some(false));
            assert_eq!(mode.state(true).stencil, StencilState::default());
        }
    }
}
