//! Explicit, bounded screen-space reflection policy.
use super::SceneValidationError;
use serde::{Deserialize, Serialize};

/// Optional scene/document reflection settings. No temporal history is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSsr {
    /// Whether reflection passes are installed.
    pub enabled: bool,
    /// Hierarchical traversal budget, within 1..=256.
    pub max_steps: u32,
    /// Maximum accepted surface depth penetration, within 1..=1000 millimetres.
    pub thickness_millimeters: u32,
    /// Surfaces at or above this roughness receive no SSR, within 1..=1000000.
    pub roughness_cutoff_micro: u32,
    /// Trace resolution: 1000000 for full or 500000 for half resolution.
    pub resolution_scale_micro: u32,
}
impl SceneSsr {
    /// Validates even disabled settings, so enabling never changes their interpretation.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for (part, observed, minimum, maximum) in [
            ("ssr max_steps", self.max_steps, 1, 256),
            (
                "ssr thickness_millimeters",
                self.thickness_millimeters,
                1,
                1000,
            ),
            (
                "ssr roughness_cutoff_micro",
                self.roughness_cutoff_micro,
                1,
                1_000_000,
            ),
        ] {
            if observed < minimum || observed > maximum {
                return Err(SceneValidationError::RangeViolation {
                    part,
                    observed: observed.into(),
                    minimum: minimum.into(),
                    maximum: maximum.into(),
                });
            }
        }
        if !matches!(self.resolution_scale_micro, 500_000 | 1_000_000) {
            return Err(SceneValidationError::RangeViolation {
                part: "ssr resolution_scale_micro",
                observed: self.resolution_scale_micro.into(),
                minimum: 500_000,
                maximum: 1_000_000,
            });
        }
        Ok(())
    }
}
