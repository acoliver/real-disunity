//! Exact signed fixed-point scalar used by every authored real-valued field.
//!
//! Authored render contracts forbid floats entirely: any non-integer quantity
//! is authored as raw micro-units (`1 / 1_000_000` scale) so decoding,
//! comparison, and re-serialization stay bit-exact.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::error::ContractDocumentError;

/// Fixed-point scale denominator of [`MicroFixed`]: one unit is `1_000_000` raw.
pub const MICRO_SCALE: i64 = 1_000_000;

/// Signed fixed-point scalar with scale `1 / 1_000_000`.
///
/// Serialized and deserialized as one bare JSON integer of raw micro-units,
/// so any fractional or non-numeric spelling is rejected by the decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MicroFixed {
    raw: i64,
}

impl MicroFixed {
    /// Zero.
    pub const ZERO: Self = Self { raw: 0 };
    /// Exactly one unit.
    pub const ONE: Self = Self { raw: MICRO_SCALE };

    /// Adopts raw micro-units verbatim.
    #[must_use]
    pub const fn from_raw(raw: i64) -> Self {
        Self { raw }
    }

    /// Returns the raw micro-unit value.
    #[must_use]
    pub const fn raw(self) -> i64 {
        self.raw
    }

    /// Returns whether the value lies inside the closed unit interval `[0, 1]`.
    #[must_use]
    pub const fn is_unit_interval(self) -> bool {
        self.raw >= 0 && self.raw <= MICRO_SCALE
    }

    /// Checked addition of two scalars.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the raw sum
    /// overflows `i64`.
    pub fn checked_add(self, other: Self) -> Result<Self, ContractDocumentError> {
        self.raw.checked_add(other.raw).map(Self::from_raw).ok_or(
            ContractDocumentError::ArithmeticOverflow {
                field: "micro scalar",
            },
        )
    }

    /// Checked subtraction of two scalars.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the raw
    /// difference overflows `i64`.
    pub fn checked_sub(self, other: Self) -> Result<Self, ContractDocumentError> {
        self.raw.checked_sub(other.raw).map(Self::from_raw).ok_or(
            ContractDocumentError::ArithmeticOverflow {
                field: "micro scalar",
            },
        )
    }

    /// Exact fixed-point product with deterministic truncation toward zero.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the scaled
    /// product leaves the `i64` raw range.
    pub fn checked_mul(self, other: Self) -> Result<Self, ContractDocumentError> {
        let product = i128::from(self.raw) * i128::from(other.raw);
        let scaled = product / i128::from(MICRO_SCALE);
        let raw = i64::try_from(scaled).map_err(|_| ContractDocumentError::ArithmeticOverflow {
            field: "micro scalar",
        })?;
        Ok(Self::from_raw(raw))
    }
}

impl Serialize for MicroFixed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.raw)
    }
}

impl<'de> Deserialize<'de> for MicroFixed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = i64::deserialize(deserializer)?;
        Ok(Self { raw })
    }
}
