use crate::PostError;

/// Transfer applied to the luminance proxy for edge decisions, never to filtered RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FxaaLuminance {
    #[default]
    Linear,
    /// Apply the sRGB OETF to linear luminance before computing contrast and gradients.
    Srgb,
}

/// Quality controls for the directional three-by-three FXAA kernel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxaaQuality {
    /// Relative contrast cutoff, finite and within 0..=1.
    pub edge_threshold: f32,
    /// Absolute contrast cutoff in the selected luminance domain, finite and within 0..=1.
    pub minimum_threshold: f32,
    /// Multiplier of the existing FXAA strength, finite and within 0..=1.
    pub subpixel_blend: f32,
    pub luminance: FxaaLuminance,
}
impl Default for FxaaQuality {
    fn default() -> Self {
        Self {
            edge_threshold: 0.1,
            minimum_threshold: 1.0 / 64.0,
            subpixel_blend: 1.0,
            luminance: FxaaLuminance::Linear,
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
        Ok(())
    }
    pub(crate) fn constants(self, strength: f32) -> [(&'static str, f64); 5] {
        [
            ("FXAA", strength.into()),
            ("EDGE_THRESHOLD", self.edge_threshold.into()),
            ("MINIMUM_THRESHOLD", self.minimum_threshold.into()),
            ("SUBPIXEL_BLEND", self.subpixel_blend.into()),
            (
                "SRGB_LUMA",
                f64::from(u8::from(self.luminance == FxaaLuminance::Srgb)),
            ),
        ]
    }
}
