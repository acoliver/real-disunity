use glam::{Mat3, Vec3};

use crate::{LightingFrame, irradiance_polynomial};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    /// Interpreted using the frame's explicit direction semantics.
    pub direction: Vec3,
    /// Linear RGB in the caller's radiometric convention; no implicit unit conversion.
    pub radiance: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AmbientLight {
    /// Ambient is supplied by a separate resource (for example a streamed environment bake).
    External,
    Hemisphere {
        sky: Vec3,
        ground: Vec3,
    },
    /// Already normalized/convolved coefficients in [1,x,y,z,xy,xz,yz,x²-y²,z²] order.
    Polynomial([Vec3; 9]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExposureOwnership {
    SceneLinear(f32),
    /// Keep the scene unexposed for metering. Authored stops remain provenance, not a multiplier.
    PostProcess {
        authored_stops: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentConfig {
    pub frame: LightingFrame,
    pub direct: DirectionalLight,
    /// Independent shadow-light direction, in the same semantics as direct lighting.
    pub shadow_direction: Option<Vec3>,
    pub ambient: AmbientLight,
    pub exposure: ExposureOwnership,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentError {
    InvalidFrame,
    InvalidDirectLight,
    InvalidShadowDirection,
    InvalidAmbient,
    InvalidExposure,
}

impl std::fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid environment configuration: {self:?}")
    }
}

impl std::error::Error for EnvironmentError {}

#[derive(Clone, Copy, Debug)]
pub struct PreparedEnvironment {
    pub frame: LightingFrame,
    pub toward_light: Vec3,
    pub toward_shadow_light: Option<Vec3>,
    pub direct_radiance: Vec3,
    pub ambient: AmbientLight,
    pub scene_exposure: f32,
    pub exposure: ExposureOwnership,
}

impl LightingFrame {
    /// A coefficient frame may include an axis reflection, but not scale or shear.
    pub fn validate(self) -> Result<Self, EnvironmentError> {
        let basis = self.coefficient_from_world;
        let gram = basis.transpose() * basis;
        if !self.up.is_finite()
            || self.up.try_normalize().is_none()
            || !basis.is_finite()
            || (gram - Mat3::IDENTITY)
                .to_cols_array()
                .iter()
                .any(|v| v.abs() > 1e-4)
        {
            return Err(EnvironmentError::InvalidFrame);
        }
        Ok(Self {
            up: self.up.normalize(),
            ..self
        })
    }
}

impl EnvironmentConfig {
    pub fn prepare(self) -> Result<PreparedEnvironment, EnvironmentError> {
        let frame = self.frame.validate()?;
        let toward_light = frame
            .direction_semantics
            .toward_light(self.direct.direction)
            .ok_or(EnvironmentError::InvalidDirectLight)?;
        if !nonnegative(self.direct.radiance) {
            return Err(EnvironmentError::InvalidDirectLight);
        }
        let toward_shadow_light = self
            .shadow_direction
            .map(|direction| {
                frame
                    .direction_semantics
                    .toward_light(direction)
                    .ok_or(EnvironmentError::InvalidShadowDirection)
            })
            .transpose()?;
        let valid_ambient = match self.ambient {
            AmbientLight::External => true,
            AmbientLight::Hemisphere { sky, ground } => nonnegative(sky) && nonnegative(ground),
            AmbientLight::Polynomial(c) => c.iter().all(|v| v.is_finite()),
        };
        if !valid_ambient {
            return Err(EnvironmentError::InvalidAmbient);
        }
        let scene_exposure = match self.exposure {
            ExposureOwnership::SceneLinear(value) if value.is_finite() && value >= 0.0 => value,
            ExposureOwnership::PostProcess { authored_stops } if authored_stops.is_finite() => 1.0,
            _ => return Err(EnvironmentError::InvalidExposure),
        };
        Ok(PreparedEnvironment {
            frame,
            toward_light,
            toward_shadow_light,
            direct_radiance: self.direct.radiance,
            ambient: self.ambient,
            scene_exposure,
            exposure: self.exposure,
        })
    }
}

impl PreparedEnvironment {
    /// Evaluate ambient only; material reflectance, AO and exposure remain separate operations.
    pub fn ambient_radiance(self, world_normal: Vec3) -> Vec3 {
        match self.ambient {
            AmbientLight::External => Vec3::ZERO,
            AmbientLight::Hemisphere { sky, ground } => {
                ground.lerp(sky, world_normal.dot(self.frame.up) * 0.5 + 0.5)
            }
            AmbientLight::Polynomial(c) => {
                irradiance_polynomial(self.frame.coefficient_from_world * world_normal, &c)
            }
        }
    }
}

fn nonnegative(value: Vec3) -> bool {
    value.is_finite() && value.cmpge(Vec3::ZERO).all()
}
