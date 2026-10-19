use serde::{Deserialize, Serialize};
use thiserror::Error;
use worldrunner_core_types::WorldPosition;

/// All world positions use metres in a right-handed Cartesian basis:
/// +X = east, +Y = elevation, +Z = south (negative north).
/// Positive rotation follows the right-hand rule.
/// Adapters must convert external axes and units before constructing a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorldCoordinateFrame {
    RightHandedYUp,
}

impl WorldCoordinateFrame {
    /// Maps named world coordinates to Cartesian XYZ metres in this frame.
    pub fn position_xyz_meters(self, position: WorldPosition) -> [f64; 3] {
        match self {
            Self::RightHandedYUp => [
                position.east_meters,
                position.elevation_meters,
                -position.north_meters,
            ],
        }
    }

    pub const fn discriminant(self) -> u8 {
        match self {
            Self::RightHandedYUp => 0,
        }
    }
}

/// Active local-to-world rotation in the world basis, as a unit quaternion [x, y, z, w].
/// Components must be finite and squared length must be within 1e-10 of one.
/// The schema does not normalize inputs or interpret external rotation encodings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentOrientation {
    pub quaternion_xyzw: [f64; 4],
}

impl EnvironmentOrientation {
    pub const IDENTITY: Self = Self {
        quaternion_xyzw: [0.0, 0.0, 0.0, 1.0],
    };

    /// Rotation about positive Y, with the angle in radians.
    /// Non-finite angles produce an invalid orientation rejected by `validate`.
    pub fn from_yaw_radians(yaw: f64) -> Self {
        let (sin, cos) = (yaw * 0.5).sin_cos();
        Self {
            quaternion_xyzw: [0.0, sin, 0.0, cos],
        }
    }

    pub fn validate(self) -> Result<(), OrientationError> {
        let length_squared: f64 = self.quaternion_xyzw.iter().map(|value| value * value).sum();
        if !self.quaternion_xyzw.iter().all(|value| value.is_finite())
            || (length_squared - 1.0).abs() > 1e-10
        {
            return Err(OrientationError::InvalidUnitQuaternion);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum OrientationError {
    #[error("orientation must be a finite unit quaternion in [x, y, z, w] order")]
    InvalidUnitQuaternion,
}

/// Caller-selected interpretation of the instance's position.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EnvironmentPlacement {
    /// Position is a ground anchor; the consumer offsets the scaled asset's lower bound
    /// along positive Y to place that bound at the anchor elevation.
    BoundsGrounded,
    /// Position is the local origin in world metres; no grounding adjustment is applied.
    Absolute,
}

impl EnvironmentPlacement {
    pub const fn discriminant(self) -> u8 {
        match self {
            Self::BoundsGrounded => 0,
            Self::Absolute => 1,
        }
    }
}
