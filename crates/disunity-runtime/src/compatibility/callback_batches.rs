use serde_json::{Value, json};

use super::CompatibilityHost;
use crate::HostError;

mod drain;
#[cfg(test)]
mod tests;

pub(super) const PROFILE: &str = "native_contextual_lifecycle_batch_v1";
const EXECUTION: &str = "native_phase_context_v1";
const CALLBACK: &str = "contextual_lifecycle_batch_v1";
const CONSUMER: &str = "managed_contextual_cleanup_batch_consumer_v1";

fn invalid(message: &str) -> HostError {
    HostError::Compatibility(format!("contextual Cleanup: {message}"))
}

fn required_profiles(required: &Value) -> Result<(), HostError> {
    for (key, expected) in [
        ("scheduler_profile", PROFILE),
        ("execution_profile", EXECUTION),
        ("callback_profile", CALLBACK),
    ] {
        if required.get(key).and_then(Value::as_str) != Some(expected) {
            return Err(invalid(
                "manifest profile selection is incomplete or unsupported",
            ));
        }
    }
    Ok(())
}

fn native_descriptor() -> Value {
    json!({
        "profile": PROFILE, "wire_version": 1, "native_only": true,
        "managed_integration": false, "callback_batch_attestation": false,
        "batch_phases": ["Cleanup"], "serial_phases": ["Initialize"],
        "max_records": 4096, "max_envelope_bytes": 1048576,
        "max_issuances_per_root": 4096, "flags": 1,
        "contacts": false, "coroutines": false, "nesting": false
    })
}

pub(super) fn validate(native: &Value, bridge: &Value, required: &Value) -> Result<(), HostError> {
    required_profiles(required)?;
    if has(required, "native_capabilities", super::terminal::NATIVE)
        || has(required, "bridge_capabilities", super::terminal::MANAGED)
    {
        return Err(invalid("terminal recovery composition is unsupported"));
    }
    if !has(native, "capabilities", PROFILE) || !has(bridge, "capabilities", CONSUMER) {
        return Err(invalid("native or managed batch capability is unavailable"));
    }
    if native.get("native_contextual_lifecycle_batch") != Some(&native_descriptor()) {
        return Err(invalid(
            "native batch descriptor does not match the consumer",
        ));
    }
    Ok(())
}

fn has(value: &Value, field: &str, expected: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(expected)))
}

pub(super) fn selected(host: &CompatibilityHost, world: u64) -> Result<bool, HostError> {
    if !host.strict.contains_key(&world) {
        return Ok(false);
    }
    let status = host
        .native
        .request(world, &json!({"op":"session.status"}))?;
    Ok(status.get("profile").and_then(Value::as_str) == Some(PROFILE))
}

fn selection_matches(required: &Value, native_selected: bool) -> Result<bool, HostError> {
    let requested = required.get("scheduler_profile").and_then(Value::as_str) == Some(PROFILE);
    if requested != native_selected {
        return Err(invalid(
            "native session and manifest profile selection differ",
        ));
    }
    if requested || required.get("callback_profile").is_some() {
        required_profiles(required)?;
    }
    Ok(requested)
}

pub(super) fn validate_selection(
    host: &CompatibilityHost,
    world: u64,
    fixture: &Value,
) -> Result<(), HostError> {
    let required = &fixture["requirements"];
    if selection_matches(required, selected(host, world)?)? {
        validate(
            &host.negotiation.native,
            &host.bridge.description()?,
            required,
        )?;
    }
    Ok(())
}

impl CompatibilityHost {
    /// Selects serial Initialize and contextual Cleanup on an owned fresh world.
    pub fn open_contextual_cleanup_phase_session(&mut self, world: u64) -> Result<(), HostError> {
        if !self.worlds.contains(&world) || self.strict.contains_key(&world) {
            return Err(invalid("requires an owned unopened world"));
        }
        let required = json!({
            "scheduler_profile": PROFILE,
            "execution_profile": EXECUTION,
            "callback_profile": CALLBACK,
        });
        validate(
            &self.negotiation.native,
            &self.bridge.description()?,
            &required,
        )?;
        self.open_selected_phase_session(world, PROFILE)
    }

    pub(super) fn reject_unsupported_contextual(&self, world: u64) -> Result<(), HostError> {
        if selected(self, world)? {
            Err(invalid(
                "contextual Cleanup supports Initialize and Cleanup only",
            ))
        } else {
            Ok(())
        }
    }
}
