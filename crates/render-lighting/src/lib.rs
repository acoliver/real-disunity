//! Independent universal reflectance kernels. Environment data and material policy belong
//! to the caller. Directions, coordinate frames, roughness floors and numerical guards are explicit.
use glam::{Mat3, Vec3};

mod environment;
pub use environment::{
    AmbientLight, DirectionalLight, EnvironmentConfig, EnvironmentError, ExposureOwnership,
    PreparedEnvironment,
};
mod shader;
pub const WGSL: &str = crate::lighting_wgsl!();

/// Visibility includes the Cook–Torrance denominator, not just the geometry term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityAlgorithm {
    SmithCorrelated,
    SchlickSmithHotness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionSemantics {
    TowardLight,
    LightTravel,
}

impl DirectionSemantics {
    pub fn toward_light(self, direction: Vec3) -> Option<Vec3> {
        if !direction.is_finite() {
            return None;
        }
        let unit = direction.try_normalize()?;
        Some(match self {
            Self::TowardLight => unit,
            Self::LightTravel => -unit,
        })
    }
}

/// Transform directions to the coefficient frame before evaluating irradiance.
#[derive(Clone, Copy, Debug)]
pub struct LightingFrame {
    pub up: Vec3,
    pub coefficient_from_world: Mat3,
    pub direction_semantics: DirectionSemantics,
}

pub fn exposure_from_stops(stops: f32) -> f32 {
    stops.exp2()
}

/// Unnormalized degree-two polynomial basis. Coefficients must already include the
/// caller's SH normalization and convolution weights; no hidden irradiance convention.
pub fn irradiance_polynomial(direction: Vec3, coefficients: &[Vec3; 9]) -> Vec3 {
    let Vec3 { x, y, z } = direction;
    let basis = [1.0, x, y, z, x * y, x * z, y * z, x * x - y * y, z * z];
    coefficients.iter().zip(basis).map(|(c, b)| *c * b).sum()
}

pub fn distribution_ggx(no_h: f32, roughness: f32, epsilon: f32) -> f32 {
    let alpha = roughness * roughness;
    let d = (no_h * no_h).mul_add(alpha * alpha - 1.0, 1.0).max(epsilon);
    (alpha / d).powi(2) / std::f32::consts::PI
}

pub fn visibility(no_v: f32, no_l: f32, roughness: f32, algorithm: VisibilityAlgorithm) -> f32 {
    match algorithm {
        VisibilityAlgorithm::SmithCorrelated => {
            let a2 = roughness.powi(4);
            let v = no_l * (no_v * no_v * (1.0 - a2) + a2).sqrt();
            let l = no_v * (no_l * no_l * (1.0 - a2) + a2).sqrt();
            0.5 / (v + l).max(1e-7)
        }
        VisibilityAlgorithm::SchlickSmithHotness => {
            let k = ((roughness + 1.0).powi(2) / 8.0).max(1e-5);
            0.25 / ((no_l * (1.0 - k) + k) * (no_v * (1.0 - k) + k))
        }
    }
}

pub fn fresnel_schlick(cosine: f32, f0: Vec3) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - cosine).clamp(0.0, 1.0).powi(5)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod math_parity_gpu_tests;

pub mod atmosphere;
pub mod ibl;

pub mod reflections;

#[cfg(test)]
mod reflections_tests;
