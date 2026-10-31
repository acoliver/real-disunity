use worldrunner_render_post::{
    CopyOutput, FrameConfig, FrameTargets, PostError, Resources, StageFactory, StageSet,
    ToneMapOutput,
};

#[derive(Clone)]
pub(super) enum FrameOutput {
    Copy(CopyOutput),
    Hdr(ToneMapOutput),
}
impl StageFactory for FrameOutput {
    fn validate_config(&self, device: &wgpu::Device, config: FrameConfig) -> Result<(), PostError> {
        match self {
            Self::Copy(factory) => factory.validate_config(device, config),
            Self::Hdr(factory) => factory.validate_config(device, config),
        }
    }
    fn frame_slots(&self) -> usize {
        match self {
            Self::Copy(_) => 1,
            Self::Hdr(f) => f.frame_slots(),
        }
    }
    fn needs_velocity(&self) -> bool {
        match self {
            Self::Copy(_) => false,
            Self::Hdr(f) => f.needs_velocity(),
        }
    }
    fn settle_build(&self, accepted: bool) {
        match self {
            Self::Copy(_) => {}
            Self::Hdr(f) => f.settle_build(accepted),
        }
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        slot: usize,
    ) -> Result<StageSet, PostError> {
        match self {
            Self::Copy(f) => f.build_for_slot(device, targets, persistent, slot),
            Self::Hdr(f) => f.build_for_slot(device, targets, persistent, slot),
        }
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError> {
        match self {
            Self::Copy(factory) => factory.build(device, targets, persistent),
            Self::Hdr(factory) => factory.build(device, targets, persistent),
        }
    }
}

/// Opt-in attachment and output policy. Scene anti-aliasing effects remain independent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FrameProfile {
    #[default]
    Legacy,
    Hdr {
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
    },
    HdrNativeRcas {
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
        rcas: worldrunner_render_post::NativeRcasSettings,
    },
    HdrUpscaled {
        samples: u32,
        tone_map: worldrunner_render_post::ToneMap,
        scale: worldrunner_render_plan::post::RenderScale,
        upscale: worldrunner_render_plan::post::UpscaleSettings,
    },
}
impl FrameProfile {
    pub const fn target(self) -> (wgpu::TextureFormat, u32) {
        match self {
            Self::Legacy => (wgpu::TextureFormat::Rgba8UnormSrgb, 1),
            Self::Hdr { samples, .. }
            | Self::HdrNativeRcas { samples, .. }
            | Self::HdrUpscaled { samples, .. } => (wgpu::TextureFormat::Rgba16Float, samples),
        }
    }
}
