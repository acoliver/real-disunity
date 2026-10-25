use super::CompatibilityHost;
use crate::HostError;
use serde_json::{Value, json};

pub(super) const PROFILE: &str = "native_host_idle_rename_v1";
const CONSUMER: &str = "managed_host_idle_rename_consumer_v1";

#[derive(Debug, thiserror::Error)]
pub enum HostIdleMutationError {
    #[error("host Idle rename rejected: {0}")]
    Rejected(HostError),
    #[error("host Idle rename outcome unknown: {primary}; cleanup errors: {cleanup:?}")]
    OutcomeUnknown {
        primary: HostError,
        cleanup: Vec<HostError>,
    },
}
fn invalid() -> HostError {
    HostError::Compatibility(
        "host Idle rename requires exact paired profile and managed scheduling".into(),
    )
}
pub(super) fn validate(native: &Value, bridge: &Value, required: &Value) -> Result<(), HostError> {
    if required["host_mutation_profile"] != PROFILE
        || required["execution_profile"] != super::phase::PROFILE
        || required.get("scheduler_profile").is_some()
    {
        return Err(invalid());
    }
    for (descriptor, capability) in [(native, PROFILE), (bridge, CONSUMER)] {
        if !descriptor["capabilities"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(capability)))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(super) fn validate_selection(
    host: &CompatibilityHost,
    world: u64,
    fixture: &Value,
) -> Result<(), HostError> {
    let status = host
        .native
        .request(world, &json!({"op":"session.status"}))?;
    let selected = status["profile"] == PROFILE;
    if selected != (fixture["requirements"]["host_mutation_profile"] == PROFILE) {
        return Err(invalid());
    }
    if selected {
        validate(
            &host.negotiation.native,
            &host.bridge.description()?,
            &fixture["requirements"],
        )?;
    }
    Ok(())
}
impl CompatibilityHost {
    pub fn open_host_idle_phase_session(&mut self, world: u64) -> Result<(), HostError> {
        self.bridge.require_idle()?;
        validate(
            &self.negotiation.native,
            &self.bridge.description()?,
            &json!({"host_mutation_profile":PROFILE,"execution_profile":super::phase::PROFILE}),
        )?;
        self.open_selected_phase_session(world, PROFILE)
    }
    pub fn rename_object_idle(
        &mut self,
        world: u64,
        object: u64,
        name: &str,
    ) -> Result<Value, HostIdleMutationError> {
        use HostIdleMutationError::Rejected;
        self.require_loaded().map_err(Rejected)?;
        self.bridge.require_idle().map_err(Rejected)?;
        let session = self.strict.get(&world).ok_or_else(|| Rejected(invalid()))?;
        if self.loaded_world != Some(world) || !session.idle_rename {
            return Err(Rejected(invalid()));
        }
        let request = json!({"version":1,"op":"host.mutate_idle","host":session.host,"request":{"op":"object.set","object":object,"name":name}});
        match self.native.request_idle_mutation(world, &request) {
            Err(HostIdleMutationError::OutcomeUnknown {
                primary,
                mut cleanup,
            }) => {
                self.strict
                    .get_mut(&world)
                    .ok_or_else(|| Rejected(invalid()))?
                    .outcome_unknown = true;
                if let Err(error) = self.unload() {
                    cleanup.push(error);
                }
                Err(HostIdleMutationError::OutcomeUnknown { primary, cleanup })
            }
            result => result,
        }
    }
    pub(super) fn require_resolved_idle(&self, world: u64) -> Result<(), HostError> {
        if self.strict.get(&world).is_some_and(|s| s.outcome_unknown) {
            return Err(HostError::Compatibility(
                "host mutation outcome unknown; cleanup-only world; replay refused".into(),
            ));
        }
        Ok(())
    }
}
