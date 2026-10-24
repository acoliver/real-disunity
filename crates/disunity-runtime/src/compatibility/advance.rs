use serde_json::{Value, json};

use super::{CompatibilityHost, RESPONSE_LIMIT};
use crate::HostError;

/// Owned contacts from one frame, including contacts preceding a reported error.
/// No response handles or native locks survive the synchronous advance.
pub struct AdvanceReport {
    pub status: Result<Value, HostError>,
    contacts: Vec<Value>,
    responses: Vec<String>,
    bytes: usize,
}

impl AdvanceReport {
    pub fn contacts(&self) -> &[Value] {
        &self.contacts
    }

    pub fn raw_contact_responses(&self) -> &[String] {
        &self.responses
    }

    fn retain(&mut self, response: String) -> Result<(), HostError> {
        let value = super::exact::response(&response)?;
        let events = value
            .as_array()
            .ok_or_else(|| HostError::Request("native contacts must be an array".into()))?;
        if self.contacts.len() + events.len() > 4096
            || response.len() > RESPONSE_LIMIT.saturating_sub(self.bytes)
        {
            return Err(HostError::Request(
                "frame contact report budget exceeded".into(),
            ));
        }
        self.bytes += response.len();
        self.contacts.extend(events.iter().cloned());
        self.responses.push(response);
        Ok(())
    }
}

impl CompatibilityHost {
    /// Preserves the native frame.end result and discards the optional report.
    pub fn advance_rational(&mut self, world: u64, num: u64, den: u64) -> Result<Value, HostError> {
        self.advance_rational_report(world, num, den).status
    }

    /// Runs one native-authorized frame and retains the same contact bytes sent
    /// to Cdecl. Application failures remain in status after all peer phases run.
    pub fn advance_rational_report(
        &mut self,
        world: u64,
        num: u64,
        den: u64,
    ) -> super::AdvanceReport {
        let mut report = AdvanceReport {
            status: Ok(Value::Null),
            contacts: Vec::new(),
            responses: Vec::new(),
            bytes: 0,
        };
        report.status = self.advance_report(world, num, den, &mut report);
        report
    }

    fn advance_report(
        &mut self,
        world: u64,
        num: u64,
        den: u64,
        report: &mut AdvanceReport,
    ) -> Result<Value, HostError> {
        self.require_loaded()?;
        if self.loaded_world != Some(world) || !self.strict.contains_key(&world) {
            return Err(HostError::Compatibility(
                "strict advance requires the loaded explicit session".into(),
            ));
        }
        self.reject_unsupported_contextual(world)?;
        let admitted =
            self.control(world, json!({"op":"frame.open","dt":{"num":num,"den":den}}))?;
        let stamp = match self.advance_admitted(world, &admitted, report) {
            Ok(stamp) => stamp,
            Err(error) => {
                let cleanup = self.unload_strict(world);
                return super::phase::join(Err(error), cleanup).map(|()| Value::Null);
            }
        };
        self.ensure_no_callback_failures()?;
        Ok(stamp)
    }

    fn advance_admitted(
        &self,
        world: u64,
        admitted: &Value,
        report: &mut AdvanceReport,
    ) -> Result<Value, HostError> {
        let steps = admitted["time"]["fixed_steps"].as_u64().ok_or_else(|| {
            HostError::Request("native frame admission omitted fixed_steps".into())
        })?;
        for _ in 0..steps {
            self.run_strict_phase(world, "FixedUpdate")?;
            self.run_strict_phase(world, "FixedCoroutines")?;
            let status = self
                .native
                .request(world, &json!({"op":"session.status"}))?;
            let tick = status["fixed_tick"]
                .as_u64()
                .and_then(|t| t.checked_add(1))
                .ok_or_else(|| HostError::Request("native fixed tick invalid".into()))?;
            self.control(
                world,
                json!({"op":"fixed.commit","frame":status["frame"],"fixed_tick":tick}),
            )?;
            self.run_contacts(world, report)?;
        }
        for phase in ["Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
            self.run_strict_phase(world, phase)?;
        }
        self.control(
            world,
            json!({"op":"frame.end","frame":admitted["time"]["frame"]}),
        )
    }

    fn run_contacts(&self, world: u64, report: &mut AdvanceReport) -> Result<(), HostError> {
        let context = self.phase_context(world, "Contacts")?;
        let called = (|| {
            let response = self
                .native
                .request_text(world, &json!({"op":"events.read","clear":false}))?;
            report.retain(response)?;
            self.bridge.invoke(
                self.bridge.contact,
                &json!({"context":context,"events_response":report.responses.last()}),
            )
        })();
        self.finish_dispatch(world, &context, called)
    }

    fn run_strict_phase(&self, world: u64, phase: &str) -> Result<(), HostError> {
        let context = self.phase_context(world, phase)?;
        let called = self
            .bridge
            .invoke(self.bridge.phase, &json!({"phase":phase,"context":context}));
        self.finish_dispatch(world, &context, called)
    }
    fn finish_dispatch(
        &self,
        world: u64,
        context: &Value,
        called: Result<(), HostError>,
    ) -> Result<(), HostError> {
        if called.is_err() && self.terminal_selected(world) {
            return called;
        }
        super::phase::join(called, self.finish_attempt(world, context))
    }
}
