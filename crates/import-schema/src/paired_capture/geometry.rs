//! Camera geometry records in the manifest's canonical units.
//!
//! Positions are metres in a named coordinate frame, orientation is a
//! normalized `x, y, z, w` quaternion, and vertical fields of view are
//! degrees. A pose whose coordinate frame is not identified cannot be
//! compared numerically and caps the comparability grade rather than being
//! silently compared.

use serde::{Deserialize, Serialize};

use super::observation::{Observed, validate_reason};
use super::{PairedCaptureError, invalid_field};
use crate::validation::{is_identifier, is_uri};

/// One camera pose in the manifest's canonical units.
///
/// Positions are metres in the frame named by `frame_id`; orientation is a normalized
/// quaternion in `x, y, z, w` order. Two actual poses can only be compared numerically when
/// their frame identifiers match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraPose {
    /// The identity of the coordinate frame this pose is expressed in.
    pub frame_id: String,
    /// The eye position in metres.
    pub position_meters: [f64; 3],
    /// The normalized orientation quaternion in `x, y, z, w` order.
    pub orientation_xyzw: [f64; 4],
}

/// The requested and the source-actual camera pose of one capture.
///
/// The requested pose is what the operator asked for and is always present; the actual pose
/// is what the source could observe about itself and may be an explicit unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseRecord {
    /// The pose the capture requested.
    pub requested: CameraPose,
    /// The pose the source actually held, when observable.
    pub actual: Observed<CameraPose>,
}

/// The projection of one capture in canonical units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRecord {
    /// The projection kind, e.g. a perspective-projection URI.
    pub projection_uri: String,
    /// The vertical field of view in degrees.
    pub vertical_fov_degrees: f64,
    /// The near plane distance in metres.
    pub near_plane_meters: f64,
    /// The far plane distance in metres.
    pub far_plane_meters: f64,
}

/// The pixel dimensions of one captured frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewportRecord {
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
}

impl CameraPose {
    /// Validates the frame identity, finite positions, and normalized orientation.
    ///
    /// `field` is the full manifest path of this pose, e.g. `pose.requested`.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed pose violation.
    pub fn validate_for(&self, field: &'static str) -> Result<(), PairedCaptureError> {
        if !is_identifier(&self.frame_id) {
            return Err(invalid_field(
                field,
                "a named coordinate frame",
                &self.frame_id,
            ));
        }
        if self.position_meters.iter().any(|value| !value.is_finite()) {
            return Err(invalid_field(field, "finite metre positions", "non-finite"));
        }
        if self.orientation_xyzw.iter().any(|value| !value.is_finite()) {
            return Err(invalid_field(
                field,
                "finite quaternion components",
                "non-finite",
            ));
        }
        let squared_norm: f64 = self
            .orientation_xyzw
            .iter()
            .map(|value| value * value)
            .sum();
        let norm = squared_norm.sqrt();
        if !norm.is_finite() || (norm - 1.0).abs() > 1.0e-6 {
            return Err(invalid_field(
                field,
                "a normalized quaternion",
                "denormalized",
            ));
        }
        Ok(())
    }
}

/// Validates one pose record including both requested and actual poses.
pub(super) fn validate_pose(pose: &PoseRecord) -> Result<(), PairedCaptureError> {
    pose.requested.validate_for("pose.requested")?;
    match &pose.actual {
        Observed::Known { value } => value.validate_for("pose.actual"),
        Observed::Unknown { reason } => validate_reason("pose.actual", reason),
    }
}

/// Validates one viewport.
pub(super) fn validate_viewport(viewport: ViewportRecord) -> Result<(), PairedCaptureError> {
    if viewport.width == 0 || viewport.height == 0 {
        return Err(invalid_field(
            "viewport",
            "positive dimensions",
            "zero dimension",
        ));
    }
    Ok(())
}

/// Validates one observed projection.
pub(super) fn validate_projection(
    projection: &Observed<ProjectionRecord>,
) -> Result<(), PairedCaptureError> {
    match projection {
        Observed::Known { value } => {
            if !is_uri(&value.projection_uri) {
                return Err(invalid_field(
                    "projection.projection_uri",
                    "projection URI",
                    &value.projection_uri,
                ));
            }
            let fov = value.vertical_fov_degrees;
            if !fov.is_finite() || fov <= 0.0 || fov >= 180.0 {
                return Err(invalid_field(
                    "projection.vertical_fov_degrees",
                    "degrees in (0, 180)",
                    "out of range",
                ));
            }
            if !value.near_plane_meters.is_finite() || value.near_plane_meters <= 0.0 {
                return Err(invalid_field(
                    "projection.near_plane_meters",
                    "finite positive metres",
                    "out of range",
                ));
            }
            if !value.far_plane_meters.is_finite()
                || value.far_plane_meters <= value.near_plane_meters
            {
                return Err(invalid_field(
                    "projection.far_plane_meters",
                    "metres greater than the near plane",
                    "out of range",
                ));
            }
            Ok(())
        }
        Observed::Unknown { reason } => validate_reason("projection", reason),
    }
}
