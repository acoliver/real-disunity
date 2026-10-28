//! One typed antialiasing choice: a multisample count plus a post resolve.
use crate::{FrameConfig, FxaaQuality, PostError, SmaaPreset, SmaaSettings};

/// Multisample count of the scene attachments.
///
/// Only 1, 2, 4 and 8 can be constructed. Whether an adapter supports a count
/// for a given format is checked before any target is allocated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MsaaSamples {
    #[default]
    X1,
    X2,
    X4,
    X8,
}
impl MsaaSamples {
    /// Every selectable count, in ascending order.
    pub const ALL: [Self; 4] = [Self::X1, Self::X2, Self::X4, Self::X8];

    pub const fn count(self) -> u32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
        }
    }
}
impl TryFrom<u32> for MsaaSamples {
    type Error = PostError;
    fn try_from(count: u32) -> Result<Self, PostError> {
        Self::ALL
            .into_iter()
            .find(|samples| samples.count() == count)
            .ok_or(PostError::SampleCount(count))
    }
}
impl From<MsaaSamples> for u32 {
    fn from(samples: MsaaSamples) -> Self {
        samples.count()
    }
}

/// FXAA strength with its quality controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxaaSettings {
    /// Blend of the entire directional resolve, finite and within 0..=1.
    pub strength: f32,
    pub quality: FxaaQuality,
}

/// Post-process antialiasing applied to the tone-mapped image.
///
/// FXAA and SMAA are alternatives: one resolve runs after tone mapping.
/// New resolves are added as variants, so match with a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum PostResolve {
    #[default]
    None,
    Fxaa(FxaaSettings),
    /// Subpixel morphological antialiasing, three passes at the render extent.
    Smaa(SmaaSettings),
}
impl PostResolve {
    /// FXAA with the default quality controls.
    pub fn fxaa(strength: f32) -> Self {
        Self::fxaa_with(strength, FxaaQuality::default())
    }

    pub fn fxaa_with(strength: f32, quality: FxaaQuality) -> Self {
        Self::Fxaa(FxaaSettings { strength, quality })
    }

    /// SMAA 1x at one of the reference quality presets.
    pub const fn smaa(preset: SmaaPreset) -> Self {
        Self::Smaa(SmaaSettings::preset(preset))
    }

    pub fn fxaa_strength(self) -> Option<f32> {
        match self {
            Self::Fxaa(fxaa) => Some(fxaa.strength),
            _ => None,
        }
    }

    /// FXAA settings that schedule a pass; zero strength is the unchanged output path.
    pub(crate) fn active_fxaa(self) -> Option<FxaaSettings> {
        match self {
            Self::Fxaa(fxaa) if fxaa.strength > 0.0 => Some(fxaa),
            _ => None,
        }
    }

    pub fn validate(self) -> Result<(), PostError> {
        match self {
            Self::None => Ok(()),
            Self::Smaa(smaa) => smaa.validate(),
            Self::Fxaa(fxaa) => {
                fxaa.quality.validate()?;
                if !fxaa.strength.is_finite() || !(0.0..=1.0).contains(&fxaa.strength) {
                    return Err(PostError::UnsupportedProfile("FXAA settings"));
                }
                Ok(())
            }
        }
    }

    /// SMAA settings that schedule passes.
    pub(crate) fn active_smaa(self) -> Option<SmaaSettings> {
        match self {
            Self::Smaa(smaa) => Some(smaa),
            _ => None,
        }
    }

    /// Whether the HDR display path must run for this resolve.
    pub(crate) fn requested(self) -> bool {
        self != Self::None
    }
}

/// The application's antialiasing choice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AntiAliasing {
    pub msaa: MsaaSamples,
    pub resolve: PostResolve,
}
impl AntiAliasing {
    pub const fn new(msaa: MsaaSamples, resolve: PostResolve) -> Self {
        Self { msaa, resolve }
    }

    pub fn validate(self) -> Result<(), PostError> {
        self.resolve.validate()
    }

    /// Scene attachment contract for this choice.
    pub const fn frame_config(self, size: [u32; 2], format: wgpu::TextureFormat) -> FrameConfig {
        FrameConfig::new(size, format, self.msaa.count())
    }
}
