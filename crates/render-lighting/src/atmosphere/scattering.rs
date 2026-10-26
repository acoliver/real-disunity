//! Fitted elevation-column scattering, metres and inverse metres.
//! O'Neil, GPU Gems 2 (2005), chapter 16 is the basis:
//! <https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering>
//! Deviations are fixed quadrature, end-of-segment camera depth, a step-coupled
//! solar fit and elevation-only columns. Phase functions are Rayleigh and
//! Cornette-Shanks (1992), Applied Optics 31, 3152-3160.
use super::AtmosphereError;
use glam::{DVec3, Vec3};

/// An elevation-column approximation with 20 fixed midpoint samples.
///
/// Columns depend only on view and solar elevation. Azimuth enters only through
/// the phase function, with no planet-shadow twilight asymmetry. Camera depth
/// includes the entire current segment. The fitted solar term reuses that
/// segment's density times step length, rather than integrating a sun path.
/// See the scene descriptor's `SceneScatteringModel::ElevationColumn20` for the
/// fitted coefficients and /255 normalization, also defined in the solar-depth fit below.
#[derive(Clone, Copy, Debug)]
pub struct ElevationColumnScattering {
    pub planet_radius: f32,
    pub outer_radius: f32,
    pub molecular: Vec3,
    pub aerosol: Vec3,
    pub scale_heights: [f32; 2],
    pub anisotropy: f32,
    pub solar_radiance: f32,
    pub solar_diameter: f32,
    pub peak_radiance: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ScatteringSample {
    pub molecular: Vec3,
    pub aerosol: Vec3,
    pub transmission: Vec3,
}
impl ElevationColumnScattering {
    pub fn validate(self) -> Result<Self, AtmosphereError> {
        if ![
            self.planet_radius,
            self.outer_radius,
            self.anisotropy,
            self.solar_radiance,
            self.solar_diameter,
            self.peak_radiance,
        ]
        .into_iter()
        .all(f32::is_finite)
            || self.planet_radius <= 0.0
            || self.outer_radius <= self.planet_radius
            || !self.molecular.is_finite()
            || !self.aerosol.is_finite()
            || self.molecular.min_element() < 0.0
            || self.aerosol.min_element() < 0.0
            || !self
                .scale_heights
                .into_iter()
                .all(|h| h.is_finite() && h > 0.0)
            || self.anisotropy.abs() >= 1.0
            || self.solar_radiance < 0.0
            || self.solar_diameter <= 0.0
            || self.peak_radiance <= 0.0
        {
            return Err(AtmosphereError);
        }
        Ok(self)
    }
    /// Elevations are radians, measured above the local horizon. Below-horizon
    /// views clamp to the final lookup row. Observer altitude is metres.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "geometry evaluated in f64 before f32 optical arithmetic"
    )]
    pub fn integrate(
        self,
        elevation: f32,
        solar_elevation: f32,
        altitude: f32,
    ) -> ScatteringSample {
        assert!(elevation.is_finite() && solar_elevation.is_finite());
        assert!(
            altitude.is_finite()
                && altitude >= 0.0
                && altitude < self.outer_radius - self.planet_radius
        );
        let angle = elevation.clamp(0.0, std::f32::consts::FRAC_PI_2);
        let origin = DVec3::Z * (f64::from(self.planet_radius) + f64::from(altitude));
        let direction = DVec3::new(f64::from(angle).cos(), 0.0, f64::from(angle).sin());
        let projection = origin.dot(direction);
        let distance = -projection
            + (f64::from(self.outer_radius).powi(2)
                - (origin.length_squared() - projection * projection))
                .sqrt();
        let step = distance / 20.0;
        let fit = self
            .scale_heights
            .map(|height| solar_depth(height, solar_elevation));
        let mut depth = [0.0; 2];
        let mut sums = [Vec3::ZERO; 2];
        let mut traveled = 0.0;
        for _ in 0..20 {
            let point = origin + direction * (traveled + 0.5 * step);
            traveled += step;
            let radius = point.length();
            let height = (radius - f64::from(self.planet_radius)) as f32;
            let density = self
                .scale_heights
                .map(|scale| (-height / scale).exp() * step as f32);
            depth = std::array::from_fn(|i| depth[i] + density[i]);
            let clearance = std::f32::consts::FRAC_PI_2
                - (f64::from(self.planet_radius) / radius)
                    .clamp(-1.0, 1.0)
                    .asin() as f32
                + (point.z / radius).clamp(-1.0, 1.0).acos() as f32
                + solar_elevation;
            let visibility = (0.5 + clearance / self.solar_diameter).clamp(0.0, 1.0);
            let extra: [f32; 2] = std::array::from_fn(|i| {
                density[i] * (fit[i][0] * angle.sin() + fit[i][1] * (-7.0 * angle).exp()) / 255.0
            });
            let attenuation = (-(self.molecular * (depth[0] + extra[0])
                + self.aerosol * 1.05 * (depth[1] + extra[1])))
                .exp();
            for i in 0..2 {
                sums[i] += density[i] * attenuation * visibility;
            }
        }
        ScatteringSample {
            molecular: sums[0],
            aerosol: sums[1],
            transmission: (-(self.molecular * depth[0] + self.aerosol * 1.05 * depth[1])).exp(),
        }
    }
    pub fn radiance(self, sample: ScatteringSample, cosine: f32) -> Vec3 {
        let color = self.solar_radiance
            * (sample.molecular * self.molecular * molecular_phase(cosine)
                + sample.aerosol * self.aerosol * aerosol_phase(cosine, self.anisotropy));
        let luminance = color.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        color * luminance.clamp(0.0, self.peak_radiance) / luminance.max(0.0001)
    }
}
fn solar_depth(height: f32, elevation: f32) -> [f32; 2] {
    let degrees = elevation.to_degrees();
    let amplitude = [
        550.6967 * height.powf(0.459_061_9),
        30.29522 * height.powf(0.453_363_9),
    ];
    if degrees < 0.0 {
        return amplitude.map(|v| v * 1.45_f32.powf(-degrees));
    }
    let divisor = 0.034_256_02 * height.powf(0.444_784);
    let factor = 1.0 + (degrees / divisor).powf(1.22);
    [
        0.042_687_92 * height - 4.773_326 + amplitude[0] / factor,
        0.002_524_803 * height - 1.745_569 + amplitude[1] / factor,
    ]
}
pub fn molecular_phase(cosine: f32) -> f32 {
    3.0 / (16.0 * std::f32::consts::PI) * (1.0 + cosine * cosine)
}
pub fn aerosol_phase(cosine: f32, anisotropy: f32) -> f32 {
    let square = anisotropy * anisotropy;
    3.0 / (8.0 * std::f32::consts::PI) * (1.0 - square) * (1.0 + cosine * cosine)
        / ((2.0 + square) * (1.0 + square - 2.0 * anisotropy * cosine).powf(1.5))
}
#[cfg(test)]
mod tests {
    use super::*;
    pub fn earth() -> ElevationColumnScattering {
        ElevationColumnScattering {
            planet_radius: 6_360_000.0,
            outer_radius: 6_420_000.0,
            molecular: Vec3::new(5.8e-6, 13.5e-6, 33.1e-6),
            aerosol: Vec3::splat(2e-6),
            scale_heights: [8000.0, 1200.0],
            anisotropy: 0.76,
            solar_radiance: 20.0,
            solar_diameter: 0.53,
            peak_radiance: 100.0,
        }
    }
    #[test]
    fn horizon_altitude_and_forward_scattering() {
        let model = earth().validate().unwrap();
        let zenith = model.integrate(std::f32::consts::FRAC_PI_2, 1.5, 0.1);
        let horizon = model.integrate(0.0, 1.5, 0.1);
        assert!(model.radiance(horizon, 0.0).length() > model.radiance(zenith, 0.0).length());
        assert!(horizon.transmission.min_element() < zenith.transmission.min_element());
        let high = model.integrate(1.0, 1.5, 10_000.0);
        assert!(high.molecular.length() < model.integrate(1.0, 1.5, 0.1).molecular.length());
        assert!(aerosol_phase(1.0, 0.76) > aerosol_phase(0.0, 0.76) * 10.0);
        let noon = model.integrate(0.0, 1.5, 0.1);
        let sunset = model.integrate(0.0, 0.0, 0.1);
        assert!(sunset.molecular.x / sunset.molecular.z > noon.molecular.x / noon.molecular.z);
    }
}

#[cfg(test)]
mod reference_tests;
