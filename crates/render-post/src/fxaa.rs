use crate::PostError;

/// Transfer applied to the luminance proxy for edge decisions, never to filtered RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FxaaLuminance {
    #[default]
    Linear,
    /// Apply the sRGB OETF to linear luminance before computing contrast and gradients.
    Srgb,
    /// Use the linear green channel directly.
    Green,
}

/// FXAA kernel and, for the quality search, its sampling policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FxaaKernel {
    #[default]
    Sobel,
    SecondDifference,
    /// Preset 12; search and final offset scales in texels, finite in (0, 4].
    /// Axial and diagonal contrast taps remain one texel apart.
    Quality {
        preset: u8,
        sample_scale: [f32; 2],
    },
}

/// Luminance, contrast and subpixel controls shared by the FXAA kernels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxaaQuality {
    /// Relative contrast cutoff, finite and within 0..=1.
    pub edge_threshold: f32,
    /// Absolute contrast cutoff in the selected luminance domain, finite and within 0..=1.
    pub minimum_threshold: f32,
    /// Subpixel amount within 0..=1; a strength multiplier for Sobel.
    pub subpixel_blend: f32,
    pub luminance: FxaaLuminance,
    pub kernel: FxaaKernel,
}
impl Default for FxaaQuality {
    fn default() -> Self {
        Self {
            edge_threshold: 0.1,
            minimum_threshold: 1.0 / 64.0,
            subpixel_blend: 1.0,
            luminance: FxaaLuminance::Linear,
            kernel: FxaaKernel::Sobel,
        }
    }
}
impl FxaaQuality {
    pub fn validate(self) -> Result<(), PostError> {
        if [
            self.edge_threshold,
            self.minimum_threshold,
            self.subpixel_blend,
        ]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(PostError::UnsupportedProfile("FXAA quality settings"));
        }
        if let FxaaKernel::Quality {
            preset,
            sample_scale,
        } = self.kernel
            && (preset != 12
                || sample_scale
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0.0 || *v > 4.0))
        {
            return Err(PostError::UnsupportedProfile(
                "FXAA quality preset or sample scale",
            ));
        }
        Ok(())
    }
    pub(crate) fn constants(self, strength: f32) -> [(&'static str, f64); 10] {
        let (preset, scale) = match self.kernel {
            FxaaKernel::Quality {
                preset,
                sample_scale,
            } => (preset, sample_scale),
            _ => (0, [1.0; 2]),
        };
        [
            ("QUALITY_PRESET", preset.into()),
            ("QUALITY_SCALE_X", scale[0].into()),
            ("QUALITY_SCALE_Y", scale[1].into()),
            ("FXAA", strength.into()),
            ("EDGE_THRESHOLD", self.edge_threshold.into()),
            ("MINIMUM_THRESHOLD", self.minimum_threshold.into()),
            ("SUBPIXEL_BLEND", self.subpixel_blend.into()),
            (
                "SRGB_LUMA",
                f64::from(u8::from(self.luminance == FxaaLuminance::Srgb)),
            ),
            (
                "GREEN_LUMA",
                f64::from(u8::from(self.luminance == FxaaLuminance::Green)),
            ),
            (
                "SECOND_DIFFERENCE",
                f64::from(u8::from(self.kernel == FxaaKernel::SecondDifference)),
            ),
        ]
    }
}
