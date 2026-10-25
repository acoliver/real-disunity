use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const MAX_OUTSTANDING: usize = 16 << 20;

pub(crate) fn effective() -> Value {
    json!({"limits":{
        "max_request_bytes":crate::response::MAX_REQUEST_BYTES,
        "max_response_bytes":crate::response::MAX_RESPONSE_BYTES,
        "max_outstanding_bytes":MAX_OUTSTANDING,
        "max_process_response_handles":crate::response::MAX_RESPONSES,
        "max_retained_requests":crate::ledger::MAX_RETAINED_REQUESTS,
        "max_retained_bytes":crate::ledger::MAX_RETAINED_BYTES,
        "retention_response_reservation_bytes":crate::response::MAX_RESPONSE_BYTES},
        "outstanding_scope":"world_generation","control_owner_quota_bytes":MAX_OUTSTANDING})
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Control,
    World(u64),
}
pub(crate) trait Admission {
    fn preflight(&mut self, _owner: Owner) -> Result<(), i32> {
        Ok(())
    }
    fn admit(&mut self, owner: Owner, bytes: &[u8]) -> Result<(), i32>;
}
pub(crate) struct Unpooled;
impl Admission for Unpooled {
    fn admit(&mut self, _owner: Owner, _bytes: &[u8]) -> Result<(), i32> {
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum Request {
    #[serde(rename = "transport.get")]
    Get {},
}
impl Request {
    pub(crate) fn is_read(&self) -> bool {
        matches!(self, Self::Get {})
    }
}
