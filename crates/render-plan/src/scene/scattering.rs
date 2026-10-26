//! Single scattering parameters in metres and scene-linear radiometric units.
use super::SceneValidationError;
use super::effects::require_range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneScatteringFormat {
    /// Finite saturation at 65504 and rounding toward zero. Below saturation,
    /// normal relative error is less than 2^-10; subnormal absolute error is less than 2^-24.
    Float16,
    /// Unquantized single-precision integrals.
    Float32,
}

/// The fitted elevation-column approximation, based on O'Neil, GPU Gems 2,
/// chapter 16. This model uses 20 fixed midpoint samples and end-of-segment
/// camera depth. Its solar depth estimate includes the view step length.
/// Lookup columns depend only on view and solar elevation. Solar azimuth enters
/// only through the phase function; there is no planet-shadow twilight asymmetry.
/// These are model definitions, so increasing lookup resolution does not make
/// the model converge to independent camera-path and sun-path optical integrals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneScatteringModel {
    /// Empirical elevation-column fit with 20 samples. For scale height H in
    /// metres and solar elevation d in degrees, let A=550.6967 H^0.4590619,
    /// B=30.29522 H^0.4533639 and F=1+(d/(0.03425602 H^0.444784))^1.22.
    /// For d>=0, S=0.04268792 H-4.773326+A/F and
    /// E=0.002524803 H-1.745569+B/F; for d<0, (S,E)=(A,B)*1.45^(-d).
    /// At view elevation v the added solar depth is
    /// exp(-height/H)*step*(S*sin(v)+E*exp(-7v))/255.
    /// The fitted coefficients and /255 normalization define the approximation;
    /// they are not an independent sun-path integral or a convergence guarantee.
    ElevationColumn20,
}

/// Scene-linear scattering using a named approximation, with metre geometry.
///
/// The elevation-only lookup ignores relative solar azimuth except in the phase
/// function and has no planet-shadow twilight asymmetry. The fitted solar term
/// is step-coupled, with end-of-segment camera depth and 20 fixed samples.
/// O'Neil is the basis, with these deviations defined by [`SceneScatteringModel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneScattering {
    pub model: SceneScatteringModel,
    /// Extinguish celestial surface radiance and add the atmospheric background.
    pub compose_celestial: bool,
    pub planet_radius_meters: u32,
    pub atmosphere_radius_meters: u32,
    /// Sea-level coefficients in trillionths per metre.
    pub molecular_scattering_pico_per_meter: [u32; 3],
    pub aerosol_scattering_pico_per_meter: [u32; 3],
    pub molecular_height_meters: u32,
    pub aerosol_height_meters: u32,
    pub anisotropy_micro: i32,
    pub solar_radiance_micro: u32,
    /// Angular diameter in millionths of a radian. The fit convention uses
    /// 530000 for 0.53 radians, without any degree conversion.
    pub solar_diameter_microradians: u32,
    pub peak_radiance_micro: u32,
    /// Uniform elevation texel centres from zenith to horizon, with clamped edges.
    /// Interpolation error depends on this resolution, separately from storage error.
    pub lookup_rows: u32,
    pub format: SceneScatteringFormat,
}
impl SceneScattering {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        for (name, value, low, high) in [
            ("planet radius", self.planet_radius_meters, 1, 100_000_000),
            (
                "atmosphere radius",
                self.atmosphere_radius_meters,
                self.planet_radius_meters.saturating_add(16),
                110_000_000,
            ),
            ("molecular height", self.molecular_height_meters, 1, 100_000),
            ("aerosol height", self.aerosol_height_meters, 1, 100_000),
            (
                "solar diameter",
                self.solar_diameter_microradians,
                1,
                3_141_592,
            ),
            ("scattering rows", self.lookup_rows, 2, 4096),
            ("scattering peak", self.peak_radiance_micro, 1, u32::MAX),
        ] {
            require_range(name, u64::from(value), u64::from(low), u64::from(high))?;
        }
        require_range(
            "scattering anisotropy",
            u64::from(self.anisotropy_micro.unsigned_abs()),
            0,
            999_000,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_geometry_and_sampling_fail_before_preparation() {
        let mut settings = SceneScattering {
            compose_celestial: false,
            planet_radius_meters: 6_360_000,
            atmosphere_radius_meters: 6_420_000,
            molecular_scattering_pico_per_meter: [5_800_000, 13_500_000, 33_100_000],
            aerosol_scattering_pico_per_meter: [2_000_000; 3],
            molecular_height_meters: 8000,
            aerosol_height_meters: 1200,
            anisotropy_micro: 760_000,
            solar_radiance_micro: 20_000_000,
            solar_diameter_microradians: 530_000,
            peak_radiance_micro: 100_000_000,
            model: SceneScatteringModel::ElevationColumn20,
            lookup_rows: 128,
            format: SceneScatteringFormat::Float16,
        };
        assert!(settings.validate().is_ok());
        settings.atmosphere_radius_meters = settings.planet_radius_meters;
        assert!(settings.validate().is_err());
        settings.planet_radius_meters = u32::MAX;
        assert!(settings.validate().is_err());
    }
}
