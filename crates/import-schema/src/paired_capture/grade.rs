//! Comparability grading: tolerance validation and fail-closed derivation.
//!
//! The grade is derived from what both sides actually recorded; it is never
//! read back from the claim. Poses are compared numerically only within the
//! same named coordinate frame, so an unidentified frame caps the grade
//! instead of being silently compared.

use serde::{Deserialize, Serialize};

use super::observation::{CORE_STATE_KEYS, Observed};
use super::side::CaptureSide;
use super::{PairedCaptureError, invalid_field};

/// How closely two captures may be compared, from weakest to strongest.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparabilityGrade {
    /// The frames do not correspond to the same input; only separate observation claims hold.
    UnpairedObservation,
    /// The inputs pair but the poses align only approximately.
    ApproximatePose,
    /// Both actual poses are recorded, registered, and within tolerance.
    PoseEquivalent,
    /// The pose-equivalent frames also hold matching known state and projection.
    StateEquivalent,
    /// The state-equivalent frames also match in viewport and outputs; ready for measurement.
    AcceptanceEligible,
}

impl ComparabilityGrade {
    /// The manifest's wire name for this grade.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnpairedObservation => "unpaired_observation",
            Self::ApproximatePose => "approximate_pose",
            Self::PoseEquivalent => "pose_equivalent",
            Self::StateEquivalent => "state_equivalent",
            Self::AcceptanceEligible => "acceptance_eligible",
        }
    }
}

/// The pose and projection slack allowed between two paired actual observations.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseTolerance {
    /// The maximum Euclidean position distance in metres.
    pub position_meters: f64,
    /// The maximum quaternion angular distance in degrees.
    pub orientation_degrees: f64,
    /// The maximum vertical field-of-view difference in degrees.
    pub vertical_fov_degrees: f64,
}

/// The comparability claim made about the pair.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparabilityClaim {
    /// The grade claimed for this pair; never stronger than the derived grade.
    pub claimed_grade: ComparabilityGrade,
    /// The tolerances backing pose and projection equivalence.
    pub pose_tolerance: PoseTolerance,
}

impl ComparabilityClaim {
    /// Validates the tolerance bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite or out-of-range tolerances.
    pub fn validate(&self) -> Result<(), PairedCaptureError> {
        let tolerance = self.pose_tolerance;
        for (field, value, maximum) in [
            (
                "pose_tolerance.position_meters",
                tolerance.position_meters,
                f64::INFINITY,
            ),
            (
                "pose_tolerance.orientation_degrees",
                tolerance.orientation_degrees,
                180.0,
            ),
            (
                "pose_tolerance.vertical_fov_degrees",
                tolerance.vertical_fov_degrees,
                180.0,
            ),
        ] {
            if !value.is_finite() || value < 0.0 || value > maximum {
                return Err(invalid_field(
                    field,
                    "finite value in [0, maximum]",
                    "out of range",
                ));
            }
        }
        Ok(())
    }
}

/// Derives the strongest comparability grade one pair's own evidence supports.
///
/// The derivation is conservative: any value that is absent, unknown, unregistered, or
/// outside tolerance caps the grade at the boundary below it, and non-finite arithmetic
/// caps rather than passes. The claimed grade may legitimately be weaker.
#[must_use]
pub(super) fn derive_grade(
    source: &CaptureSide,
    replay: &CaptureSide,
    tolerance: PoseTolerance,
) -> ComparabilityGrade {
    if source.input_sha256 != replay.input_sha256 {
        return ComparabilityGrade::UnpairedObservation;
    }
    if !poses_pair_within_tolerance(source, replay, tolerance) {
        return ComparabilityGrade::ApproximatePose;
    }
    if !projections_pair_within_tolerance(source, replay, tolerance) {
        return ComparabilityGrade::PoseEquivalent;
    }
    if !states_match(source, replay) {
        return ComparabilityGrade::PoseEquivalent;
    }
    if source.viewport != replay.viewport {
        return ComparabilityGrade::StateEquivalent;
    }
    ComparabilityGrade::AcceptanceEligible
}

/// Returns whether both actual poses are known, registered in one frame, and within the
/// pose tolerance.
fn poses_pair_within_tolerance(
    source: &CaptureSide,
    replay: &CaptureSide,
    tolerance: PoseTolerance,
) -> bool {
    let (
        Observed::Known {
            value: source_actual,
        },
        Observed::Known {
            value: replay_actual,
        },
    ) = (&source.pose.actual, &replay.pose.actual)
    else {
        return false;
    };
    if source_actual.frame_id != replay_actual.frame_id {
        return false;
    }
    let position = distance_meters(source_actual.position_meters, replay_actual.position_meters);
    let orientation = quaternion_angle_degrees(
        source_actual.orientation_xyzw,
        replay_actual.orientation_xyzw,
    );
    within(position, tolerance.position_meters)
        && within(orientation, tolerance.orientation_degrees)
}

/// Returns whether both known projections' vertical fields of view pair within tolerance.
fn projections_pair_within_tolerance(
    source: &CaptureSide,
    replay: &CaptureSide,
    tolerance: PoseTolerance,
) -> bool {
    let (
        Observed::Known {
            value: source_projection,
        },
        Observed::Known {
            value: replay_projection,
        },
    ) = (&source.projection, &replay.projection)
    else {
        return false;
    };
    let fov =
        (source_projection.vertical_fov_degrees - replay_projection.vertical_fov_degrees).abs();
    within(fov, tolerance.vertical_fov_degrees)
}

/// Returns whether every core state key is known on both sides with equal values.
fn states_match(source: &CaptureSide, replay: &CaptureSide) -> bool {
    for key in CORE_STATE_KEYS {
        let (
            Some(Observed::Known {
                value: source_value,
            }),
            Some(Observed::Known {
                value: replay_value,
            }),
        ) = (source.state.get(key), replay.state.get(key))
        else {
            return false;
        };
        if source_value != replay_value {
            return false;
        }
    }
    true
}

/// Returns whether a computed difference is finite and within tolerance.
fn within(difference: f64, tolerance: f64) -> bool {
    difference.is_finite() && tolerance.is_finite() && difference <= tolerance
}

/// Returns the Euclidean distance between two positions in metres.
fn distance_meters(left: [f64; 3], right: [f64; 3]) -> f64 {
    let deltas = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    (deltas[0] * deltas[0] + deltas[1] * deltas[1] + deltas[2] * deltas[2]).sqrt()
}

/// Returns the angular distance between two quaternions in degrees.
fn quaternion_angle_degrees(left: [f64; 4], right: [f64; 4]) -> f64 {
    let dot = (left[0] * right[0] + left[1] * right[1] + left[2] * right[2] + left[3] * right[3])
        .abs()
        .min(1.0);
    (2.0 * dot.acos()).to_degrees()
}
