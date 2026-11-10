use wgpu::{
    CompareFunction, DepthBiasState, DepthStencilState, StencilFaceState, StencilOperation,
    StencilState, TextureFormat,
};

/// The depth modes of the fixed path, as `renderer::depth::DepthMode` has them. The terrain
/// shading is frozen; the depth contract follows the renderer's tie rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Depth {
    Independent,
    Shared,
    Prepass,
    Reuse,
}

impl From<crate::renderer::depth::DepthMode> for Depth {
    fn from(mode: crate::renderer::depth::DepthMode) -> Self {
        use crate::renderer::depth::DepthMode;
        match mode {
            DepthMode::Independent => Self::Independent,
            DepthMode::Shared => Self::Shared,
            DepthMode::Prepass => Self::Prepass,
            DepthMode::Reuse => Self::Reuse,
        }
    }
}

impl Depth {
    pub(crate) const fn format(self) -> TextureFormat {
        match self {
            Self::Independent => TextureFormat::Depth32Float,
            Self::Shared | Self::Prepass | Self::Reuse => TextureFormat::Depth32FloatStencil8,
        }
    }

    /// The depth and stencil state of a terrain pipeline, which always writes depth.
    pub(super) fn state(self) -> DepthStencilState {
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
            depth_write_enabled: Some(self != Self::Reuse),
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
            bias: DepthBiasState::default(),
        }
    }
}
