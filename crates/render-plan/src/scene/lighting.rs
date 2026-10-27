//! Scene-wide lighting choices. All coefficients are linear, before exposure.
use super::{SceneEffects, SceneValidationError};

/// Prepared degree-two diffuse irradiance divided by pi, in millionths.
/// Coefficients multiply [1,x,y,z,xy,xz,yz,x²-y²,z²] in scene space (+Z up).
/// Negative coefficients are valid; evaluation clamps the final RGB to zero.
/// Multiplication by diffuse albedo produces outgoing radiance with no further pi factor.
/// Ramamoorthi and Hanrahan (2001): <https://graphics.stanford.edu/papers/envmap/>.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSh9([[i32; 3]; 9]);
impl SceneSh9 {
    /// Adopts prepared coefficients, bounded to +/-1024 per channel.
    pub fn new(coefficients_micro: [[i32; 3]; 9]) -> Result<Self, SceneValidationError> {
        if coefficients_micro
            .iter()
            .flatten()
            .any(|c| c.unsigned_abs() > 1_024_000_000)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "SH9 coefficient range",
            });
        }
        Ok(Self(coefficients_micro))
    }
    pub const fn coefficients_micro(self) -> [[i32; 3]; 9] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneDiffuseEnvironment {
    #[default]
    Hemisphere,
    Sh9(SceneSh9),
}

impl SceneEffects {
    /// Selects a validated linear RGB multiplier for the scene sun only.
    #[must_use]
    pub fn with_sun_tint(mut self, tint: super::SceneTint) -> Self {
        self.sun_tint = tint;
        self
    }
    pub const fn sun_tint(&self) -> super::SceneTint {
        self.sun_tint
    }
    /// Selects diffuse lighting without changing sun, exposure or specular lighting.
    #[must_use]
    pub fn with_diffuse_environment(mut self, environment: SceneDiffuseEnvironment) -> Self {
        self.diffuse_environment = environment;
        self
    }
    pub const fn diffuse_environment(&self) -> SceneDiffuseEnvironment {
        self.diffuse_environment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_coefficients_accept_signed_values_and_reject_extremes() {
        assert!(SceneSh9::new([[-1_024_000_000; 3]; 9]).is_ok());
        assert!(SceneSh9::new([[1_024_000_001; 3]; 9]).is_err());
        assert!(SceneSh9::new([[i32::MIN; 3]; 9]).is_err());
        assert_eq!(
            SceneDiffuseEnvironment::default(),
            SceneDiffuseEnvironment::Hemisphere
        );
    }
}

/// Material roughness interpretation, analytic highlight floor and GGX distribution guard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneRoughnessRemap {
    /// Legacy D guard: alpha² / max(pi * denominator², 1e-7).
    #[default]
    Perceptual045,
    /// Analytic floor 0.04; D = (alpha / max(denominator, 1e-8))² / pi.
    Perceptual04,
    /// Input is GGX alpha rather than perceptual roughness. Floor is 0.04.
    /// Uses the same denominator guard as Perceptual04, independently of visibility.
    LinearAlpha,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneSpecularVisibility {
    #[default]
    SmithCorrelated,
    /// Karis (2013): hotness reduction for analytic lights only, not the IBL LUT.
    SchlickSmith,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneSpecularOcclusion {
    #[default]
    AmbientOnly,
    /// Minimum of material AO and screen AO, sun relief, roughness-aware specular AO.
    RoughnessAware,
}
/// Direct-light radiometry. Prepared lights already include Lambert's normalization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneDirectDiffuse {
    #[default]
    EnergyConserving,
    LambertianPrepared,
}

/// A fixed 0.0027-radian disk with independent scene-linear highlight intensity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSurfaceDisk {
    intensity_micro: u32,
    peak_micro: u32,
}
impl SceneSurfaceDisk {
    pub fn new(intensity_micro: u32, peak_micro: u32) -> Result<Self, SceneValidationError> {
        super::effects::require_range(
            "surface highlight peak",
            u64::from(peak_micro),
            1,
            u64::from(u32::MAX),
        )?;
        Ok(Self {
            intensity_micro,
            peak_micro,
        })
    }
    pub const fn intensity_micro(self) -> u32 {
        self.intensity_micro
    }
    pub const fn peak_micro(self) -> u32 {
        self.peak_micro
    }
}

/// Scene-wide BRDF policy. Default preserves the existing render output.
/// Visibility follows Heitz (2014), <https://jcgt.org/published/0003/02/03/>, or
/// Karis (2013), <https://cdn2.unrealengine.com/Resources/files/2013SiggraphPresentationsNotes-26915738.pdf>.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneSpecularConvention {
    pub direct_diffuse: SceneDirectDiffuse,
    pub surface_disk: Option<SceneSurfaceDisk>,
    pub roughness: SceneRoughnessRemap,
    pub visibility: SceneSpecularVisibility,
    pub occlusion: SceneSpecularOcclusion,
}
impl SceneSpecularConvention {
    /// Numeric selections shared by pipeline constants and environment preparation.
    pub const fn shader_options(self) -> [u32; 6] {
        let roughness = match self.roughness {
            SceneRoughnessRemap::Perceptual045 => 0,
            SceneRoughnessRemap::Perceptual04 => 1,
            SceneRoughnessRemap::LinearAlpha => 2,
        };
        let visibility = match self.visibility {
            SceneSpecularVisibility::SmithCorrelated => 0,
            SceneSpecularVisibility::SchlickSmith => 1,
        };
        let occlusion = match self.occlusion {
            SceneSpecularOcclusion::AmbientOnly => 0,
            SceneSpecularOcclusion::RoughnessAware => 1,
        };
        let diffuse = match self.direct_diffuse {
            SceneDirectDiffuse::EnergyConserving => 0,
            SceneDirectDiffuse::LambertianPrepared => 1,
        };
        let (intensity, peak) = match self.surface_disk {
            None => (0, 0),
            Some(disk) => (disk.intensity_micro, disk.peak_micro),
        };
        [roughness, visibility, occlusion, diffuse, intensity, peak]
    }
}
impl SceneEffects {
    #[must_use]
    pub fn with_specular_convention(mut self, convention: SceneSpecularConvention) -> Self {
        self.specular_convention = convention;
        self
    }
    pub const fn specular_convention(&self) -> SceneSpecularConvention {
        self.specular_convention
    }
}
