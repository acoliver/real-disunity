//! Validated unit directions; scene consumers use the right-handed Z-up basis.

use crate::scene::error::SceneValidationError;
use serde::{Deserialize, Serialize};

/// Scale of a unit direction's integer components.
pub const DIRECTION_MICRO_SCALE: i32 = 1_000_000;

/// Unit direction toward a light source, quantized to micro-units.
///
/// Construction accepts squared-norm error up to `2 * scale + 1`, covering
/// independent rounding of three unit-vector components. Components cannot
/// be changed after validation. A direction has no quaternion scalar part.
/// Scene suns interpret XYZ as east, north, and elevation; document suns use
/// their authored coordinate basis. This type validates the norm only and
/// never converts between bases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "[i32; 3]", into = "[i32; 3]")]
pub struct SceneDirection {
    components_micro: [i32; 3],
}

impl SceneDirection {
    /// Constructs a quantized unit vector without repairing invalid inputs.
    pub fn new(components_micro: [i32; 3]) -> Result<Self, SceneValidationError> {
        let squared: i128 = components_micro.iter().map(|&v| i128::from(v).pow(2)).sum();
        let scale = i128::from(DIRECTION_MICRO_SCALE);
        if (squared - scale * scale).abs() > 2 * scale + 1 {
            return Err(SceneValidationError::DirectionNotNormalized);
        }
        Ok(Self { components_micro })
    }

    /// Returns XYZ micro-components in the caller's basis; for scene suns these
    /// are east, north, and elevation.
    pub const fn components_micro(self) -> [i32; 3] {
        self.components_micro
    }
}

impl TryFrom<[i32; 3]> for SceneDirection {
    type Error = SceneValidationError;

    fn try_from(components: [i32; 3]) -> Result<Self, Self::Error> {
        Self::new(components)
    }
}

impl From<SceneDirection> for [i32; 3] {
    fn from(direction: SceneDirection) -> Self {
        direction.components_micro()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_directions_validate_and_have_no_quaternion_part() {
        let direction = SceneDirection::new([0, 0, 1_000_000]).unwrap();
        let bytes = serde_json::to_vec(&direction).unwrap();
        assert_eq!(bytes, b"[0,0,1000000]");
        assert_eq!(
            serde_json::from_slice::<SceneDirection>(&bytes).unwrap(),
            direction
        );
        for invalid in [
            "[0,0,0]",
            "[0,0,500000]",
            "[0,0,0,1000000]",
            "{\"x\":0,\"y\":0,\"z\":0,\"w\":1000000}",
        ] {
            assert!(serde_json::from_str::<SceneDirection>(invalid).is_err());
        }
    }

    #[test]
    fn accepts_signed_axes_and_rounded_unit_directions() {
        for components in [
            [1_000_000, 0, 0],
            [0, -1_000_000, 0],
            [0, 0, 1_000_000],
            [577_350, -577_350, 577_350],
            [707_107, 707_107, 0],
        ] {
            assert_eq!(
                SceneDirection::new(components).unwrap().components_micro(),
                components
            );
        }
    }

    #[test]
    fn rejects_zero_short_and_long_directions() {
        for components in [[0; 3], [0, 0, 500_000], [1_000_002, 0, 0]] {
            assert_eq!(
                SceneDirection::new(components),
                Err(SceneValidationError::DirectionNotNormalized)
            );
        }
    }

    #[test]
    fn extreme_direction_norms_return_validation_errors() {
        for components in [
            [i32::MAX; 3],
            [i32::MIN; 3],
            [i32::MIN, i32::MAX, i32::MIN],
            [0, 0, i32::MIN],
        ] {
            assert_eq!(
                SceneDirection::new(components),
                Err(SceneValidationError::DirectionNotNormalized)
            );
        }
    }
}
