use glam::DVec3;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

use crate::ViewerError;

/// Explicit Z-up orbit bounds, in scene millimetres and radians.
#[derive(Clone, Copy, Debug)]
pub struct OrbitLimits {
    /// At least 10 mm, keeping integer camera poses away from coincident eyes.
    pub min_distance_millimeters: f64,
    pub max_distance_millimeters: f64,
    /// Elevation bounds must be ordered within `-1.47..=1.47`, away from roll poles.
    pub min_elevation_radians: f64,
    pub max_elevation_radians: f64,
}

/// Deterministic display-free orbit policy. Invalid/nonfinite input is ignored.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    initial: CameraPose,
    pose: CameraPose,
    limits: OrbitLimits,
    distance: f64,
    azimuth: f64,
    elevation: f64,
}

impl OrbitCamera {
    pub fn new(pose: CameraPose, limits: OrbitLimits) -> Result<Self, ViewerError> {
        if !limits.min_distance_millimeters.is_finite()
            || !limits.max_distance_millimeters.is_finite()
            || limits.min_distance_millimeters < 10.0
            || limits.max_distance_millimeters < limits.min_distance_millimeters
            || limits.max_distance_millimeters > 1.0e12
            || !limits.min_elevation_radians.is_finite()
            || !limits.max_elevation_radians.is_finite()
            || limits.min_elevation_radians < -1.47
            || limits.max_elevation_radians > 1.47
            || limits.min_elevation_radians > limits.max_elevation_radians
        {
            return Err(ViewerError::InvalidOptions("invalid orbit limits"));
        }
        let delta = vector(pose.eye_millimeters) - vector(pose.target_millimeters);
        let distance = delta.length();
        let elevation = (delta.z / distance).asin();
        if !distance.is_finite()
            || !elevation.is_finite()
            || distance < limits.min_distance_millimeters
            || distance > limits.max_distance_millimeters
            || elevation < limits.min_elevation_radians
            || elevation > limits.max_elevation_radians
        {
            return Err(ViewerError::InvalidOptions(
                "initial pose is outside orbit limits",
            ));
        }
        Ok(Self {
            initial: pose,
            pose,
            limits,
            distance,
            azimuth: delta.y.atan2(delta.x),
            elevation,
        })
    }

    pub const fn pose(&self) -> CameraPose {
        self.pose
    }

    pub fn reset(&mut self) {
        // Construction already proved the original pose is valid and in bounds.
        *self = Self::new(self.initial, self.limits).expect("validated initial orbit");
    }

    /// Applies pixel motion normalized to the current viewport, not frame time.
    /// Zero-area viewports and nonfinite deltas leave the camera unchanged.
    pub fn drag(&mut self, delta: [f64; 2], viewport: Viewport) {
        if viewport.width == 0 || viewport.height == 0 || delta.iter().any(|v| !v.is_finite()) {
            return;
        }
        self.azimuth = (self.azimuth
            - delta[0].clamp(-1.0e6, 1.0e6) / f64::from(viewport.width) * std::f64::consts::TAU)
            .rem_euclid(std::f64::consts::TAU);
        self.elevation = (self.elevation
            + delta[1].clamp(-1.0e6, 1.0e6) / f64::from(viewport.height) * std::f64::consts::PI)
            .clamp(
                self.limits.min_elevation_radians,
                self.limits.max_elevation_radians,
            );
        self.update_pose();
    }

    /// Positive wheel steps move toward the target, with bounded exponential zoom.
    pub fn zoom(&mut self, steps: f64) {
        if !steps.is_finite() {
            return;
        }
        self.distance = (self.distance * (-steps.clamp(-100.0, 100.0) * 0.12).exp()).clamp(
            self.limits.min_distance_millimeters,
            self.limits.max_distance_millimeters,
        );
        self.update_pose();
    }

    pub fn view(&self, camera: SceneCamera, viewport: Viewport) -> Result<SceneView, ViewerError> {
        Ok(SceneView::new(self.pose, camera, viewport)?)
    }

    fn update_pose(&mut self) {
        let horizontal = self.distance * self.elevation.cos();
        let offset = DVec3::new(
            horizontal * self.azimuth.cos(),
            horizontal * self.azimuth.sin(),
            self.distance * self.elevation.sin(),
        );
        self.pose.eye_millimeters = integers(vector(self.initial.target_millimeters) + offset);
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "orbit math is floating point; final view validation detects unrepresentable poses"
)]
fn vector(value: [i64; 3]) -> DVec3 {
    DVec3::new(value[0] as f64, value[1] as f64, value[2] as f64)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "integer scene API requires millimetre rounding; Rust casts saturate at i64 bounds"
)]
fn integers(value: DVec3) -> [i64; 3] {
    value.to_array().map(|v| v.round() as i64)
}

#[cfg(test)]
mod tests;
