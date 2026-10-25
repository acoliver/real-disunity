use crate::transport::{Admission, Owner};
use crate::{DisunityWorld, encoding::encode, ledger::Ledger, protocol, result_code};
use serde_json::Value;
use std::thread::{ThreadId, current};

pub(crate) const WRONG_THREAD: &str = "world mutation requires its owner thread";
pub(crate) struct Session {
    pub(crate) world: DisunityWorld,
    owner: ThreadId,
    ledger: Ledger,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            world: DisunityWorld::default(),
            owner: current().id(),
            ledger: Ledger::default(),
        }
    }
}
impl Session {
    pub(crate) fn require_owner(&self) -> Result<(), &'static str> {
        if self.owner != current().id() {
            return Err(WRONG_THREAD);
        }
        Ok(())
    }
    fn execute_admitted(
        &mut self,
        world: u64,
        envelope: protocol::Envelope,
        bytes: &[u8],
        admit: &mut impl Admission,
    ) -> Result<Vec<u8>, i32> {
        let owner = Owner::World(world);
        admit.preflight(owner)?;
        if !envelope.is_unretained_read() && self.require_owner().is_err() {
            return admit_response(
                encode(
                    Err(WRONG_THREAD.into()),
                    crate::response::MAX_RESPONSE_BYTES,
                )?,
                owner,
                admit,
            );
        }
        if let Some(id) = &envelope.id {
            match self.ledger.replay(id, bytes) {
                Ok(Some(response)) => return admit_response(response, owner, admit),
                Err(error) => {
                    return admit_response(
                        encode(Err(error), crate::response::MAX_RESPONSE_BYTES)?,
                        owner,
                        admit,
                    );
                }
                Ok(None) => self.ledger.reserve(id, bytes)?,
            }
        }
        let prepared = prepare_response(&mut self.world, envelope.request, world)?;
        admit.admit(owner, &prepared.response)?;
        if let Some(staged) = prepared.world {
            self.world = staged;
        }
        if let Some(id) = envelope.id {
            self.ledger.retain(id, bytes, &prepared.response);
        }
        Ok(prepared.response)
    }
}

pub(crate) fn execute(world: u64, value: Value, bytes: &[u8]) -> Result<Vec<u8>, i32> {
    execute_prepared_admitted(
        world,
        protocol::Envelope::parse(value),
        bytes,
        &mut crate::transport::Unpooled,
    )
}
pub(crate) fn execute_prepared_admitted(
    world: u64,
    parsed: Result<protocol::Envelope, String>,
    bytes: &[u8],
    admit: &mut impl Admission,
) -> Result<Vec<u8>, i32> {
    if world == 0
        && let Some(data) = parsed
            .as_ref()
            .ok()
            .and_then(protocol::Envelope::global_descriptor)
    {
        let owner = Owner::Control;
        admit.preflight(owner)?;
        return admit_response(
            encode(Ok(data), crate::response::MAX_RESPONSE_BYTES)?,
            owner,
            admit,
        );
    }
    let mut store = crate::lock_store();
    let session = match crate::live_session_mut(&mut store, world) {
        Ok(session) => session,
        Err(_) => {
            admit.preflight(Owner::Control)?;
            return Err(result_code::INVALID_HANDLE);
        }
    };
    match parsed {
        Ok(envelope) => session.execute_admitted(world, envelope, bytes, admit),
        Err(error) => {
            let owner = Owner::World(world);
            admit.preflight(owner)?;
            admit_response(
                encode(Err(error), crate::response::MAX_RESPONSE_BYTES)?,
                owner,
                admit,
            )
        }
    }
}
struct PreparedResponse {
    response: Vec<u8>,
    world: Option<DisunityWorld>,
}
impl PreparedResponse {
    fn unchanged(response: Vec<u8>) -> Self {
        Self {
            response,
            world: None,
        }
    }
}
fn admit_response(
    response: Vec<u8>,
    owner: Owner,
    admit: &mut impl Admission,
) -> Result<Vec<u8>, i32> {
    admit.admit(owner, &response)?;
    Ok(response)
}
fn prepare_response(
    world: &mut DisunityWorld,
    parsed: Result<protocol::Request, String>,
    identity: u64,
) -> Result<PreparedResponse, i32> {
    let limit = crate::response::MAX_RESPONSE_BYTES;
    let request = match parsed {
        Ok(request) => request,
        Err(error) => return encode(Err(error), limit).map(PreparedResponse::unchanged),
    };
    if request.is_read() {
        return bounded_response(request.execute_with_limit(world, identity, limit), limit)
            .map(PreparedResponse::unchanged);
    }
    let changes = match request.changes_content() {
        Ok(changes) => changes,
        Err(error) => return encode(Err(error), limit).map(PreparedResponse::unchanged),
    };
    let mut staged = world.clone();
    if changes && let Err(error) = staged.advance_content() {
        return encode(Err(error.into()), limit).map(PreparedResponse::unchanged);
    }
    let result = request.execute_with_limit(&mut staged, identity, limit);
    let success = result.is_ok();
    let response = match encode(result, limit) {
        Ok(response) => response,
        Err(result_code::BUDGET) => return budget_response(limit).map(PreparedResponse::unchanged),
        Err(code) => return Err(code),
    };
    Ok(PreparedResponse {
        response,
        world: success.then_some(staged),
    })
}
fn bounded_response(result: Result<Value, String>, limit: usize) -> Result<Vec<u8>, i32> {
    match encode(result, limit) {
        Err(result_code::BUDGET) => budget_response(limit),
        result => result,
    }
}
fn budget_response(limit: usize) -> Result<Vec<u8>, i32> {
    encode(Err(crate::response::RESPONSE_BUDGET_EXCEEDED.into()), limit)
}

#[cfg(test)]
#[path = "integrity/compat.rs"]
mod compat;
#[cfg(test)]
#[path = "integrity/idle_mutation_tests.rs"]
mod idle_mutation_tests;
#[cfg(test)]
#[path = "integrity/tests.rs"]
mod tests;
