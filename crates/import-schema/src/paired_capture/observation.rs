//! Explicit known/unknown observation values and the required state contract.
//!
//! Every state variable the contract requires must be addressed, either as a
//! known value or as an explicit unknown with a reason; omission is never
//! silent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{PairedCaptureError, invalid_field};
use crate::validation::{is_identifier, is_nonempty_text};

/// State variables every paired capture must address, each as a known value or an explicit
/// unknown with a reason.
pub const CORE_STATE_KEYS: [&str; 9] = [
    "animation_frame",
    "exposure",
    "graphics",
    "layers",
    "time_of_day",
    "turn",
    "ui",
    "weather",
    "world",
];

/// A value that was either observed or could not be observed, with the reason.
///
/// Unknowns are first-class: a manifest that could not read a value records why instead of
/// omitting the field, so downstream comparison can see exactly what it does not know.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observed<T> {
    /// The value was observed.
    Known {
        /// The observed value.
        value: T,
    },
    /// The value could not be observed.
    Unknown {
        /// Why the value is unavailable.
        reason: String,
    },
}

/// Named observation state, from required core keys to source-specific extras.
pub type CaptureState = BTreeMap<String, Observed<String>>;

/// Validates one named state map, requiring every core key to be addressed.
pub(super) fn validate_state(state: &CaptureState) -> Result<(), PairedCaptureError> {
    for key in state.keys() {
        if !is_identifier(key) {
            return Err(invalid_field("state", "identifier key", key));
        }
    }
    for key in CORE_STATE_KEYS {
        match state.get(key) {
            Some(Observed::Known { value }) => {
                if !is_nonempty_text(value) {
                    return Err(invalid_field("state", "nonempty known value", key));
                }
            }
            Some(Observed::Unknown { reason }) => validate_reason("state", reason)?,
            None => {
                return Err(invalid_field(
                    "state",
                    "an entry for every core state key",
                    key,
                ));
            }
        }
    }
    Ok(())
}

/// Validates one observed free-text value.
pub(super) fn validate_observed_text(
    field: &'static str,
    observed: &Observed<String>,
) -> Result<(), PairedCaptureError> {
    match observed {
        Observed::Known { value } => {
            if !is_nonempty_text(value) {
                return Err(invalid_field(field, "nonempty known value", value));
            }
            Ok(())
        }
        Observed::Unknown { reason } => validate_reason(field, reason),
    }
}

/// Validates one unknown's reason.
pub(super) fn validate_reason(field: &'static str, reason: &str) -> Result<(), PairedCaptureError> {
    if !is_nonempty_text(reason) {
        return Err(invalid_field(field, "nonempty unknown reason", reason));
    }
    Ok(())
}
