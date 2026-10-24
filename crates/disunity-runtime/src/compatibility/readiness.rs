mod nested;

use serde_json::{Value, json};

use crate::HostError;

pub(super) use nested::validate_selection;

pub(super) const PROFILE: &str = "native_readiness_single_v1";

fn invalid() -> HostError {
    HostError::Compatibility(
        "compatibility handshake: incompatible bounded readiness profile".into(),
    )
}

pub(super) fn validate(native: &Value, bridge: &Value, required: &Value) -> Result<(), HostError> {
    if required["scheduler_profile"] == nested::PROFILE {
        nested::validate(native, bridge)?;
    } else if required["scheduler_profile"] != PROFILE {
        return Err(invalid());
    }
    if required["execution_profile"] != super::phase::PROFILE
        || !has(native, "capabilities", "native_readiness_primitives_v1")
        || !has(bridge, "capabilities", PROFILE)
    {
        return Err(invalid());
    }
    let descriptor = &native["native_readiness"];
    for (key, value) in [
        ("capability", json!("native_readiness_primitives_v1")),
        ("requires_profile", json!(super::phase::PROFILE)),
        ("iterator_model", json!("single_iterator_per_routine")),
        ("max_claims_per_root", json!(4096)),
        ("max_scripts", json!(4096)),
        ("max_routines", json!(4096)),
    ] {
        if descriptor[key] != value {
            return Err(invalid());
        }
    }
    for wait in ["next", "scaled", "unscaled", "fixed", "end_frame", "join"] {
        if !has(descriptor, "waits", wait) {
            return Err(invalid());
        }
    }
    Ok(())
}

fn has(value: &Value, field: &str, expected: &str) -> bool {
    value[field]
        .as_array()
        .is_some_and(|values| values.contains(&json!(expected)))
}
