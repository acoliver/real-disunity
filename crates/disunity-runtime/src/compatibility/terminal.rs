use serde_json::{Value, json};

use super::CompatibilityHost;
use crate::HostError;

pub(super) const NATIVE: &str = "native_terminal_cleanup_v1";
pub(super) const MANAGED: &str = "managed_terminal_cleanup_v1";

#[derive(Default, PartialEq)]
pub(super) enum CleanupState {
    #[default]
    Legacy,
    Ready,
    Attempted,
}

pub(super) fn required(fixture: &Value) -> Result<bool, HostError> {
    let requirements = &fixture["requirements"];
    let has = |key: &str, name: &str| {
        requirements[key]
            .as_array()
            .is_some_and(|values| values.contains(&json!(name)))
    };
    let native = has("native_capabilities", NATIVE);
    let managed = has("bridge_capabilities", MANAGED);
    if native != managed || (native && requirements["execution_profile"] != super::phase::PROFILE) {
        return Err(HostError::Compatibility(
            "terminal cleanup requires paired native/managed capabilities and strict execution"
                .into(),
        ));
    }
    Ok(native)
}

impl CompatibilityHost {
    pub(super) fn terminal_selected(&self, world: u64) -> bool {
        self.strict
            .get(&world)
            .is_some_and(|s| s.cleanup != CleanupState::Legacy)
    }

    pub(super) fn terminal_context(&mut self, world: u64) -> Result<Value, HostError> {
        self.bridge.require_idle()?;
        let session = self.strict.get_mut(&world).ok_or(HostError::NoWorld)?;
        if session.cleanup != CleanupState::Ready {
            return Err(HostError::Compatibility(
                "cleanup_incomplete: terminal dispatch already attempted; no retry".into(),
            ));
        }
        // Consume before transport: an uncertain receipt never authorizes replay.
        session.cleanup = CleanupState::Attempted;
        let status = self.request(world, &json!({"op":"session.status"}))?;
        if status["stage"] == "Closed" {
            return Err(HostError::Compatibility(
                "cleanup_unavailable_closed".into(),
            ));
        }
        let result = self
            .control(
                world,
                json!({"op":"session.begin_cleanup","reason":"infrastructure_failure"}),
            )
            .map_err(|error| {
                HostError::Compatibility(format!("cleanup_admission_or_transport_failed: {error}"))
            })?;
        result.get("context").cloned().ok_or_else(|| {
            HostError::Compatibility(
                "cleanup_transport_uncertain: private context unavailable; no retry".into(),
            )
        })
    }

    pub(super) fn measured_empty(&self) -> Result<bool, HostError> {
        let roots = self.observe(&json!({"op":"managed.statistics"}))?;
        Ok(roots["data"]["loaded"] == false
            && ["scripts", "coroutines", "bindings"]
                .iter()
                .all(|key| roots["data"][key] == 0))
    }
}
