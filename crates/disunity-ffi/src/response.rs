//! Owned response buffers for execute-once native requests.
use crate::{
    result_code,
    transport::{Admission, Owner, Profile},
};
use std::sync::Mutex;
pub const MAX_REQUEST_BYTES: usize = 1 << 20;
pub const MAX_RESPONSE_BYTES: usize = 16 << 20;
pub const MAX_LIVE_RESPONSE_BYTES: usize = 64 << 20;
pub const MAX_RESPONSES: usize = 1024;
pub(crate) const RESPONSE_BUDGET_EXCEEDED: &str = "response budget exceeded; operation rolled back";
struct Response {
    handle: u64,
    bytes: Vec<u8>,
    owner: Owner,
}
struct Responses {
    slots: Vec<Option<Response>>,
    bytes: usize,
}
static RESPONSES: Mutex<Responses> = Mutex::new(Responses {
    slots: Vec::new(),
    bytes: 0,
});
struct PoolAdmission<'a> {
    pool: &'a Responses,
    owner: Option<Owner>,
}
impl Admission for PoolAdmission<'_> {
    fn preflight(&mut self, owner: Owner) -> Result<(), i32> {
        if self.pool.slots.iter().flatten().count() >= MAX_RESPONSES
            || (owner.profile == Profile::Legacy
                && self.pool.bytes + MAX_RESPONSE_BYTES > MAX_LIVE_RESPONSE_BYTES)
        {
            return Err(result_code::BUDGET);
        }
        Ok(())
    }
    fn admit(&mut self, owner: Owner, bytes: &[u8]) -> Result<(), i32> {
        self.preflight(owner)?;
        let (used, limit) = match owner.profile {
            Profile::Legacy => (self.pool.bytes, MAX_LIVE_RESPONSE_BYTES),
            Profile::Target => (
                self.pool
                    .slots
                    .iter()
                    .flatten()
                    .filter(|response| response.owner == owner)
                    .map(|response| response.bytes.len())
                    .sum(),
                crate::transport::MAX_OUTSTANDING,
            ),
        };
        if bytes.len() > owner.profile.reply_limit() || bytes.len() > limit.saturating_sub(used) {
            return Err(result_code::BUDGET);
        }
        self.owner = Some(owner);
        Ok(())
    }
}

/// Executes once with atomic admission. Semantic failures are JSON responses.
pub fn request_bytes(world: u64, request: &[u8]) -> Result<u64, i32> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(result_code::BUDGET);
    }
    let value = serde_json::from_slice(request).map_err(|_| result_code::INVALID_ARGUMENT)?;
    let parsed = crate::protocol::Envelope::parse(value);
    let mut responses = RESPONSES.lock().unwrap();
    if responses.slots.iter().flatten().count() >= MAX_RESPONSES {
        return Err(result_code::BUDGET);
    }
    let index = responses
        .slots
        .iter()
        .position(Option::is_none)
        .unwrap_or(responses.slots.len());
    if index == responses.slots.len() {
        responses
            .slots
            .try_reserve(1)
            .map_err(|_| result_code::BUDGET)?;
    }
    let mut admission = PoolAdmission {
        pool: &responses,
        owner: None,
    };
    let bytes =
        crate::integrity::execute_prepared_admitted(world, parsed, request, &mut admission)?;
    let owner = admission
        .owner
        .expect("successful execution admitted its response");
    let handle = crate::make_handle(index, crate::next_generation());
    if owner.profile == Profile::Legacy {
        responses.bytes += bytes.len();
    }
    let response = Some(Response {
        handle,
        bytes,
        owner,
    });
    if index == responses.slots.len() {
        responses.slots.push(response);
    } else {
        responses.slots[index] = response;
    }
    Ok(handle)
}
/// Copies an immutable owned response. Reading never executes gameplay.
pub fn response_bytes(response: u64) -> Result<Vec<u8>, i32> {
    let responses = RESPONSES.lock().unwrap();
    let i = (response & 0xffffffff) as usize;
    responses
        .slots
        .get(i)
        .and_then(Option::as_ref)
        .filter(|r| r.handle == response)
        .map(|r| r.bytes.clone())
        .ok_or(result_code::INVALID_HANDLE)
}
/// Releases a response independently of its world lifetime.
pub fn response_free(response: u64) -> Result<(), i32> {
    let mut responses = RESPONSES.lock().unwrap();
    let i = (response & 0xffffffff) as usize;
    if !responses
        .slots
        .get(i)
        .and_then(Option::as_ref)
        .is_some_and(|r| r.handle == response)
    {
        return Err(result_code::INVALID_HANDLE);
    }
    let r = responses.slots[i].take().unwrap();
    if r.owner.profile == Profile::Legacy {
        responses.bytes -= r.bytes.len();
    }
    Ok(())
}
