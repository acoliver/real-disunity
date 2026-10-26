//! Validated thin-surface sun scattering parameters.
use super::{SceneValidationError, material_features::bounded};

/// Quantized parameters for a metalness-channel transmission mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTranslucencyParts {
    pub distortion_micro: u32,
    pub falloff_micro: u32,
    pub sun_scale_micro: u32,
    pub ambient_scale_micro: u32,
    pub scale_micro: u32,
    pub base_kelvin: u32,
    pub warming_kelvin: u32,
    pub luminance_micro: u32,
    pub saturation_micro: u32,
}

/// A validated metalness-channel mask mode; metalness becomes zero before lighting.
/// Let `a = min(texture_ao, vertex_ao)` where absent texture AO and disabled
/// vertex AO each contribute one, and let `f` be the material AO factor.
/// Self-occlusion is `a * f`. A texture-mapped mask transmits
/// `channel * a * a * f`; a scalar mask transmits `channel * a * f`.
/// The material factor therefore participates exactly once in either mode.
/// The mapped channel includes the material metalness scalar before AO coupling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTranslucency(SceneTranslucencyParts);
impl SceneTranslucency {
    pub fn new(parts: SceneTranslucencyParts) -> Result<Self, SceneValidationError> {
        for (name, value, maximum) in [
            ("transmission distortion", parts.distortion_micro, 4_000_000),
            ("transmission falloff", parts.falloff_micro, 64_000_000),
            ("transmission sun scale", parts.sun_scale_micro, 16_000_000),
            (
                "transmission ambient scale",
                parts.ambient_scale_micro,
                16_000_000,
            ),
            ("transmission scale", parts.scale_micro, 16_000_000),
            ("transmission temperature", parts.base_kelvin, 40_000),
            ("transmission warming", parts.warming_kelvin, 40_000),
            ("transmission luminance", parts.luminance_micro, 16_000_000),
            ("transmission saturation", parts.saturation_micro, 4_000_000),
        ] {
            bounded(name, value, maximum)?;
        }
        if parts.base_kelvin < 1000 || parts.falloff_micro == 0 {
            return Err(SceneValidationError::UnsupportedInput {
                part: "transmission requires temperature >= 1000 K and positive falloff",
            });
        }
        Ok(Self(parts))
    }
    pub const fn parts(self) -> SceneTranslucencyParts {
        self.0
    }
}
