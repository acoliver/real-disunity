//! One typed antialiasing choice: a multisample count plus a post resolve.
use crate::{
    CasSettings, Cmaa2Preset, Cmaa2Settings, FrameConfig, FxaaQuality, PostError, ProjectionJitter,
    SmaaPreset, SmaaSettings, TaaSettings,
};

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
/// FXAA, SMAA and CMAA2 are alternatives: one resolve runs after tone mapping. TAA is the
/// exception that runs before tone mapping; it is still one choice among the alternatives.
/// New resolves are added as variants, so match with a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum PostResolve {
    #[default]
    None,
    Fxaa(FxaaSettings),
    /// Subpixel morphological antialiasing, three passes at the render extent.
    Smaa(SmaaSettings),
    /// Conservative morphological antialiasing, compute passes at the render extent.
    Cmaa2(Cmaa2Settings),
    /// Temporal accumulation of jittered frames, run before tone mapping. It selects
    /// projection jitter automatically; see [`PostResolve::effective_jitter`].
    Taa(TaaSettings),
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

    /// CMAA2 single-sample at one of the reference quality presets.
    pub const fn cmaa2(preset: Cmaa2Preset) -> Self {
        Self::Cmaa2(Cmaa2Settings::preset(preset))
    }

    /// TAA with the given history weight and the default clip box.
    pub const fn taa(history_weight: f32) -> Self {
        Self::Taa(TaaSettings::new(history_weight))
    }

    pub const fn taa_with(settings: TaaSettings) -> Self {
        Self::Taa(settings)
    }

    /// Temporal settings, present only for the TAA resolve.
    pub fn active_taa(self) -> Option<TaaSettings> {
        match self {
            Self::Taa(taa) => Some(taa),
            _ => None,
        }
    }

    /// The projection jitter this resolve renders with, given the explicitly requested one.
    ///
    /// TAA needs jitter, so it supplies the default sequence when none was requested and
    /// keeps an explicit choice. Every other resolve renders unjittered unless jitter was
    /// requested. This is the one place that rule lives.
    pub fn effective_jitter(self, explicit: Option<ProjectionJitter>) -> Option<ProjectionJitter> {
        match (self, explicit) {
            (Self::Taa(_), None) => Some(ProjectionJitter::default()),
            _ => explicit,
        }
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
            Self::Cmaa2(cmaa2) => cmaa2.validate(),
            Self::Taa(taa) => taa.validate(),
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

    /// Whether a resolve pass is scheduled, which needs the display-referred intermediate.
    pub(crate) fn schedules_pass(self) -> bool {
        self.active_fxaa().is_some()
            || self.active_smaa().is_some()
            || self.active_cmaa2().is_some()
    }

    /// CMAA2 settings that schedule passes.
    pub(crate) fn active_cmaa2(self) -> Option<Cmaa2Settings> {
        match self {
            Self::Cmaa2(cmaa2) => Some(cmaa2),
            _ => None,
        }
    }

    /// Format of the display-referred image between tone mapping and the resolve.
    ///
    /// FXAA keeps `Rgba32Float`, its flat-region precision contract. SMAA reads values in
    /// 0..1 with an 11-bit mantissa, finer than the 8-bit output, and its filterable
    /// `Rgba16Float` input halves the tone stage's store bandwidth and allows a hardware
    /// bilinear fetch in neighborhood blending. CMAA2 uses `Rgba16Float` for the same reason and
    /// because it is a core write-only storage format, so its working copy needs no adapter
    /// feature.
    pub(crate) fn display_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Smaa(_) | Self::Cmaa2(_) => wgpu::TextureFormat::Rgba16Float,
            _ => wgpu::TextureFormat::Rgba32Float,
        }
    }

    /// Whether the HDR display path must run for this resolve.
    pub(crate) fn requested(self) -> bool {
        self != Self::None
    }
}

/// The application's antialiasing choice.
///
/// `sharpen` is independent of the resolve: it runs after FXAA, SMAA or, with `PostResolve::None`,
/// directly after tone mapping. `None` schedules no pass and leaves the output unchanged.
///
/// `jitter` selects sub-pixel projection jitter for the scene passes. It is off by default, and
/// then every output is byte-identical to a renderer without the field. Jitter alone shimmers:
/// it exists for the temporal resolve that accumulates the jittered frames, and `PostResolve::Taa`
/// turns it on by itself. Read the jitter a renderer uses through [`Self::effective_jitter`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AntiAliasing {
    pub msaa: MsaaSamples,
    pub resolve: PostResolve,
    pub sharpen: Option<CasSettings>,
    pub jitter: Option<ProjectionJitter>,
}
impl AntiAliasing {
    /// A choice without sharpening.
    pub const fn new(msaa: MsaaSamples, resolve: PostResolve) -> Self {
        Self {
            msaa,
            resolve,
            sharpen: None,
            jitter: None,
        }
    }

    /// This choice with sub-pixel projection jitter. See the type documentation: without a
    /// temporal resolve the jittered image shimmers.
    #[must_use]
    pub const fn with_jitter(self, jitter: ProjectionJitter) -> Self {
        Self {
            jitter: Some(jitter),
            ..self
        }
    }

    /// This choice with contrast adaptive sharpening after the resolve.
    #[must_use]
    pub const fn with_sharpen(self, sharpen: CasSettings) -> Self {
        Self {
            sharpen: Some(sharpen),
            ..self
        }
    }

    /// The jitter the scene passes use: the explicit choice, or the default sequence under TAA.
    pub fn effective_jitter(self) -> Option<ProjectionJitter> {
        self.resolve.effective_jitter(self.jitter)
    }

    pub fn validate(self) -> Result<(), PostError> {
        self.resolve.validate()?;
        self.sharpen.map_or(Ok(()), CasSettings::validate)
    }

    /// Scene attachment contract for this choice.
    pub const fn frame_config(self, size: [u32; 2], format: wgpu::TextureFormat) -> FrameConfig {
        FrameConfig::new(size, format, self.msaa.count())
    }
}
