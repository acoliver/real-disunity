use serde_json::{Value, json};

use super::CompatibilityHost;
use crate::HostError;

pub(super) const PROFILE: &str = "native_phase_context_v1";

// This authority is never serialized, cloned into manifests, or formatted in errors.
pub(super) struct StrictSession {
    pub(super) host: Value,
    pub(super) idle_rename: bool,
    pub(super) outcome_unknown: bool,
    pub(super) cleanup: super::terminal::CleanupState,
}

fn invalid(message: &str) -> HostError {
    HostError::Compatibility(format!("strict phase adapter: {message}"))
}

pub(super) fn join(
    first: Result<(), HostError>,
    cleanup: Result<(), HostError>,
) -> Result<(), HostError> {
    match (first, cleanup) {
        (Err(first), Err(cleanup)) => Err(HostError::Compatibility(format!(
            "{first}; cleanup: {cleanup}"
        ))),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

impl CompatibilityHost {
    /// Explicitly selects native phase-context authorization on an owned fresh world.
    pub fn open_phase_session(&mut self, world: u64) -> Result<(), HostError> {
        self.open_selected_phase_session(world, PROFILE)
    }

    pub(super) fn open_selected_phase_session(
        &mut self,
        world: u64,
        profile: &str,
    ) -> Result<(), HostError> {
        if !self.worlds.contains(&world) || self.strict.contains_key(&world) {
            return Err(invalid("requires an owned unopened world"));
        }
        if !self.negotiation.native["capabilities"]
            .as_array()
            .is_some_and(|a| a.contains(&json!("native_phase_context_native_v1")))
        {
            return Err(invalid(
                "native phase-context provider capability is unavailable",
            ));
        }
        let bridge = self.bridge.description()?;
        for capability in [PROFILE, "strict_contact_envelope_native_json_v1"] {
            if !bridge["capabilities"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(capability)))
            {
                return Err(invalid(
                    "managed strict contact consumer capability is unavailable",
                ));
            }
        }
        let opened = self
            .native
            .request(world, &json!({"op":"session.open","profile":profile}))?;
        let session = opened["session"]
            .as_str()
            .ok_or_else(|| invalid("session identity missing"))?;
        let key = opened["host_key"]
            .as_str()
            .ok_or_else(|| invalid("private authority missing"))?;
        self.strict.insert(
            world,
            StrictSession {
                idle_rename: profile == super::idle_mutation::PROFILE,
                outcome_unknown: false,
                host: json!({"session":session,"host_key":key}),
                cleanup: super::terminal::CleanupState::Legacy,
            },
        );
        Ok(())
    }

    pub(super) fn control(&self, world: u64, mut request: Value) -> Result<Value, HostError> {
        let authority = self
            .strict
            .get(&world)
            .ok_or_else(|| invalid("world has no explicit phase session"))?;
        request["host"] = authority.host.clone();
        self.native.request(world, &request)
    }

    pub(super) fn control_text(&self, world: u64, mut request: Value) -> Result<String, HostError> {
        let authority = self
            .strict
            .get(&world)
            .ok_or_else(|| invalid("world has no explicit phase session"))?;
        request["host"] = authority.host.clone();
        self.native.request_text(world, &request)
    }

    pub(super) fn phase_context(&self, world: u64, phase: &str) -> Result<Value, HostError> {
        let status = self
            .native
            .request(world, &json!({"op":"session.status"}))?;
        let tick = status["fixed_tick"]
            .as_u64()
            .ok_or_else(|| invalid("native fixed tick missing"))?;
        let tick = if matches!(phase, "FixedUpdate" | "FixedCoroutines") {
            tick.checked_add(1)
                .ok_or_else(|| invalid("fixed tick overflow"))?
        } else {
            tick
        };
        let result = self.control(
            world,
            json!({"op":"phase.open","phase":phase,"frame":status["frame"],"fixed_tick":tick}),
        )?;
        result
            .get("context")
            .cloned()
            .ok_or_else(|| invalid("native context missing"))
    }

    pub(super) fn finish_attempt(&self, world: u64, context: &Value) -> Result<(), HostError> {
        let finished = self.control(world, json!({"op":"phase.finish","context":context}));
        if let Err(error) = finished {
            if self.terminal_selected(world) {
                return Err(error);
            }
            // Unclaimed roots can occur when parsing/binding/transport fails before entry.
            let _ = self.control(world, json!({"op":"phase.cancel","context":context}));
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn load_strict(&mut self, world: u64, fixture: &Value) -> Result<(), HostError> {
        super::readiness::validate_selection(self, world, fixture)?;
        self.validate_byte_manifest(world, fixture)?;
        if fixture["requirements"]["execution_profile"] != PROFILE {
            return Err(invalid(
                "manifest must explicitly require native_phase_context_v1",
            ));
        }
        if super::terminal::required(fixture)? {
            self.strict
                .get_mut(&world)
                .ok_or(HostError::NoWorld)?
                .cleanup = super::terminal::CleanupState::Ready;
        }
        let context = self.phase_context(world, "Initialize")?;
        let mut manifest = fixture.clone();
        manifest["initial_context"] = context.clone();
        self.loaded = true;
        self.loaded_world = Some(world);
        let called = self.bridge.invoke(self.bridge.load, &manifest);
        let finished = if called.is_err() && self.terminal_selected(world) {
            Ok(())
        } else {
            self.finish_attempt(world, &context)
        };
        if let Err(error) = join(called, finished) {
            let cleanup = self.unload();
            return join(Err(error), cleanup);
        }
        self.ensure_no_callback_failures()
    }

    pub(super) fn close_phase_session(&self, world: u64) -> Result<(), HostError> {
        let status = self
            .native
            .request(world, &json!({"op":"session.status"}))?;
        if status["stage"] == "Closed" {
            return Ok(());
        }
        self.control(world, json!({"op":"session.close"}))
            .map(|_| ())
    }
}
