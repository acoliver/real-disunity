//! Exact coordinate basis, orientation, and transform types.
//!
//! All lengths are authored in whole millimeters, all dimensionless ratios in
//! micro-units, and all orientations as exactly normalized integer
//! quaternions, so a document admits one exact interpretation.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::numeric::MICRO_SCALE;
use crate::document::validate::{checked_add_i64, require_positive_u32, require_range};

/// Handedness of one authored coordinate basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handedness {
    /// Right-handed basis.
    RightHanded,
    /// Left-handed basis.
    LeftHanded,
}

/// One signed axis direction of a coordinate basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisDirection {
    /// The positive X half-axis.
    PositiveX,
    /// The negative X half-axis.
    NegativeX,
    /// The positive Y half-axis.
    PositiveY,
    /// The negative Y half-axis.
    NegativeY,
    /// The positive Z half-axis.
    PositiveZ,
    /// The negative Z half-axis.
    NegativeZ,
}

impl AxisDirection {
    /// Returns whether two directions name exactly opposite half-axes.
    #[must_use]
    pub const fn is_opposite(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::PositiveX, Self::NegativeX)
                | (Self::NegativeX, Self::PositiveX)
                | (Self::PositiveY, Self::NegativeY)
                | (Self::NegativeY, Self::PositiveY)
                | (Self::PositiveZ, Self::NegativeZ)
                | (Self::NegativeZ, Self::PositiveZ)
        )
    }
}

/// Authored coordinate basis: handedness, orientation axes, and unit length.
///
/// The basis is the single authority for interpreting every authored
/// position; documents cannot mix bases implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinateBasis {
    /// Handedness of the authored axes.
    pub handedness: Handedness,
    /// Axis the authored up direction maps to.
    pub up_axis: AxisDirection,
    /// Axis the authored forward direction maps to.
    pub forward_axis: AxisDirection,
    /// Length of one authored unit in millimeters.
    pub unit_millimeters: u32,
}

impl CoordinateBasis {
    /// Validates the basis invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the unit is not strictly
    /// positive or the up and forward axes are equal or opposite.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_positive_u32("basis unit_millimeters", self.unit_millimeters)?;
        if self.up_axis == self.forward_axis || self.up_axis.is_opposite(self.forward_axis) {
            return Err(ContractDocumentError::ViolatedInvariant {
                field: "basis up_axis/forward_axis",
                reason: "up and forward axes must be distinct and not opposite",
            });
        }
        Ok(())
    }
}

/// Exact authored position in whole millimeters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionMillimeters {
    /// Signed X coordinate in millimeters.
    pub x: i64,
    /// Signed Y coordinate in millimeters.
    pub y: i64,
    /// Signed Z coordinate in millimeters.
    pub z: i64,
}

impl PositionMillimeters {
    /// Adopts the three signed millimeter coordinates.
    #[must_use]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    /// Checked component-wise addition.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when any
    /// component sum overflows `i64`.
    pub fn checked_add(
        self,
        other: Self,
        field: &'static str,
    ) -> Result<Self, ContractDocumentError> {
        Ok(Self {
            x: checked_add_i64(field, self.x, other.x)?,
            y: checked_add_i64(field, self.y, other.y)?,
            z: checked_add_i64(field, self.z, other.z)?,
        })
    }
}

/// Exactly normalized unit rotation quaternion with micro-unit components.
///
/// The four raw components are authored in micro-units and must satisfy
/// `x*x + y*y + z*z + w*w == MICRO_SCALE * MICRO_SCALE` exactly, so every
/// orientation admits exactly one exact unit quaternion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationQuaternion {
    /// Raw X component in micro-units.
    pub x: i64,
    /// Raw Y component in micro-units.
    pub y: i64,
    /// Raw Z component in micro-units.
    pub z: i64,
    /// Raw W component in micro-units.
    pub w: i64,
}

impl RotationQuaternion {
    /// The identity rotation.
    pub const IDENTITY: Self = Self {
        x: 0,
        y: 0,
        z: 0,
        w: MICRO_SCALE,
    };

    /// Validates exact unit normalization.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the component squares do not
    /// sum to exactly one squared micro-unit scale.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        let scale = i128::from(MICRO_SCALE);
        let squares = i128::from(self.x) * i128::from(self.x)
            + i128::from(self.y) * i128::from(self.y)
            + i128::from(self.z) * i128::from(self.z)
            + i128::from(self.w) * i128::from(self.w);
        if squares == scale * scale {
            Ok(())
        } else {
            Err(ContractDocumentError::ViolatedInvariant {
                field: "rotation quaternion",
                reason: "components must satisfy x*x + y*y + z*z + w*w == 1e12 exactly",
            })
        }
    }
}

/// Strictly positive uniform scale authored in millionths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniformScaleMillionths {
    /// Raw scale in millionths; must be at least 1.
    pub millionths: u64,
}

impl UniformScaleMillionths {
    /// Exact unit scale.
    pub const ONE: Self = Self {
        millionths: 1_000_000,
    };

    /// Adopts raw millionths.
    #[must_use]
    pub const fn from_raw(millionths: u64) -> Self {
        Self { millionths }
    }

    /// Returns the raw millionths value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.millionths
    }

    /// Validates strict positivity.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::OutOfRange`] for zero scales, which
    /// would collapse instances to a point.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "uniform scale millionths",
            1,
            i128::from(u64::MAX),
            i128::from(self.millionths),
        )
    }

    /// Checked composition with deterministic truncation toward zero.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the scaled
    /// product leaves the `u64` range.
    pub fn checked_mul(self, other: Self) -> Result<Self, ContractDocumentError> {
        let product = u128::from(self.millionths) * u128::from(other.millionths);
        let scaled = product / u128::from(1_000_000_u64);
        let millionths =
            u64::try_from(scaled).map_err(|_| ContractDocumentError::ArithmeticOverflow {
                field: "uniform scale millionths",
            })?;
        Ok(Self { millionths })
    }
}

/// Authored rigid placement: exact position, exact rotation, uniform scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformMillimeters {
    /// Position in whole millimeters under the document basis.
    pub position: PositionMillimeters,
    /// Exactly normalized orientation.
    pub rotation: RotationQuaternion,
    /// Strictly positive uniform scale in millionths.
    pub scale: UniformScaleMillionths,
}

impl TransformMillimeters {
    /// Validates the rotation normalization and scale positivity.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when either child field is invalid.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        self.rotation.validate()?;
        self.scale.validate()
    }
}
