//! Scene-linear sky radiance and the Beer-Lambert integral of exponential height fog.
use glam::Vec3;

/// Compose a water medium with an already fogged opaque snapshot. The camera
/// segment is applied only to newly added radiance, including when opaque.
pub fn compose_water_fog(
    snapshot: Vec3,
    medium_transmission: Vec3,
    scattering: Vec3,
    specular: Vec3,
    camera_transmission: f32,
    fog_radiance: Vec3,
) -> Vec3 {
    snapshot * medium_transmission
        + (scattering * camera_transmission + fog_radiance * (1.0 - camera_transmission))
            * (Vec3::ONE - medium_transmission)
        + specular * camera_transmission
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtmosphereError;
impl std::fmt::Display for AtmosphereError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid atmospheric parameters")
    }
}
impl std::error::Error for AtmosphereError {}

/// Radiance values are linear RGB, with no exposure or display transfer function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyGradient {
    pub horizon: Vec3,
    pub zenith: Vec3,
    pub exponent: f32,
}
impl SkyGradient {
    pub fn new(horizon: Vec3, zenith: Vec3, exponent: f32) -> Result<Self, AtmosphereError> {
        if !positive_color(horizon)
            || !positive_color(zenith)
            || !exponent.is_finite()
            || exponent <= 0.0
        {
            return Err(AtmosphereError);
        }
        Ok(Self {
            horizon,
            zenith,
            exponent,
        })
    }
    /// Directions and up must be unit length in the same coordinate frame.
    pub fn radiance(self, direction: Vec3, up: Vec3) -> Vec3 {
        self.horizon.lerp(
            self.zenith,
            direction.dot(up).clamp(0.0, 1.0).powf(self.exponent),
        )
    }
}

/// Extinction is per metre at `base_height`; height falloff is inverse metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeightFog {
    pub density: f32,
    pub falloff: f32,
    pub base_height: f32,
    pub start_distance: f32,
    pub radiance: Vec3,
}
impl HeightFog {
    pub fn new(
        density: f32,
        falloff: f32,
        base_height: f32,
        start_distance: f32,
        radiance: Vec3,
    ) -> Result<Self, AtmosphereError> {
        if ![density, falloff, base_height, start_distance]
            .iter()
            .all(|x| x.is_finite())
            || density < 0.0
            || falloff < 0.0
            || start_distance < 0.0
            || !positive_color(radiance)
        {
            return Err(AtmosphereError);
        }
        Ok(Self {
            density,
            falloff,
            base_height,
            start_distance,
            radiance,
        })
    }
    /// `offset` runs from eye to surface in metres; up is unit length.
    pub fn transmittance(self, eye: Vec3, offset: Vec3, up: Vec3) -> f32 {
        let length = offset.length();
        if length <= self.start_distance || self.density == 0.0 {
            return 1.0;
        }
        let direction = offset / length;
        let start = eye + direction * self.start_distance;
        let distance = length - self.start_distance;
        let height = start.dot(up) - self.base_height;
        let x = self.falloff * direction.dot(up) * distance;
        let log_integral = if x.abs() < 0.001 {
            (1.0 - x * 0.5 + x * x / 6.0 - x * x * x / 24.0).ln()
        } else if x < -50.0 {
            -x + (-x.exp()).ln_1p() - (-x).ln()
        } else {
            (-(-x).exp_m1() / x).ln()
        };
        let log_depth = self.density.ln() - self.falloff * height + distance.ln() + log_integral;
        if log_depth >= 5.0 {
            0.0
        } else {
            (-log_depth.exp()).exp()
        }
    }
    pub fn apply(self, radiance: Vec3, eye: Vec3, offset: Vec3, up: Vec3) -> Vec3 {
        self.radiance
            .lerp(radiance, self.transmittance(eye, offset, up))
    }
}
fn positive_color(value: Vec3) -> bool {
    value.is_finite() && value.cmpge(Vec3::ZERO).all()
}

pub mod scattering;
