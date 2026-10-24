use serde_json::{Value, json};

use super::{has, invalid};
use crate::HostError;
use crate::compatibility::CompatibilityHost;

pub(super) const PROFILE: &str = "native_readiness_nested_start_v1";
const CONSUMER: &str = "managed_immediate_nested_start_consumer_v1";

pub(super) fn validate(native: &Value, bridge: &Value) -> Result<(), HostError> {
    if !has(native, "capabilities", PROFILE) || !has(bridge, "capabilities", CONSUMER) {
        return Err(invalid());
    }
    let descriptor = &native["native_readiness_nested_start"];
    for (key, value) in [
        ("capability", json!(PROFILE)),
        ("requires_profile", json!(PROFILE)),
        ("max_child_depth", json!(32)),
        ("max_claims_per_root", json!(4096)),
        ("claim_reservation_per_child_scope", json!(3)),
        ("immediate_child_starts", json!(true)),
        ("start_inside_lifecycle", json!(true)),
        ("start_inside_movenext", json!(true)),
        ("start_inside_current", json!(false)),
        ("start_inside_dispose", json!(false)),
        ("yielded_ienumerator", json!(false)),
        ("predicates", json!(false)),
    ] {
        if descriptor[key] != value {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(in crate::compatibility) fn validate_selection(
    host: &CompatibilityHost,
    world: u64,
    fixture: &Value,
) -> Result<(), HostError> {
    super::super::callback_batches::validate_selection(host, world, fixture)?;
    if super::super::callback_batches::selected(host, world)? {
        return Ok(());
    }
    let status = host.request(world, &json!({"op":"session.status"}))?;
    let selected = status["profile"] == PROFILE;
    let required = fixture["requirements"]["scheduler_profile"] == PROFILE;
    if selected != required {
        return Err(invalid());
    }
    Ok(())
}

impl CompatibilityHost {
    /// Selects bounded native immediate child starts before world materialization.
    pub fn open_nested_phase_session(&mut self, world: u64) -> Result<(), HostError> {
        validate(&self.negotiation.native, &self.bridge.description()?)?;
        self.open_selected_phase_session(world, PROFILE)
    }
}

#[cfg(test)]
mod tests;
