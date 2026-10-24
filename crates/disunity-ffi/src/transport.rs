use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const PROFILE: &str = "transport_byte_limits_v1";
pub(crate) const MAX_REPLY: usize = 8 << 20;
pub(crate) const MAX_OUTSTANDING: usize = 16 << 20;
pub(crate) const MAX_RETAINED: usize = 16 << 20;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Profile {
    #[default]
    Legacy,
    Target,
}
impl Profile {
    pub(crate) fn reply_limit(self) -> usize {
        match self {
            Self::Legacy => crate::response::MAX_RESPONSE_BYTES,
            Self::Target => MAX_REPLY,
        }
    }
    pub(crate) fn retained_limit(self) -> usize {
        match self {
            Self::Legacy => crate::ledger::MAX_RETAINED_BYTES,
            Self::Target => MAX_RETAINED,
        }
    }
    pub(crate) fn effective(self) -> Value {
        let (name, outstanding, scope) = match self {
            Self::Legacy => (
                "legacy",
                crate::response::MAX_LIVE_RESPONSE_BYTES,
                "process_legacy_pool",
            ),
            Self::Target => (PROFILE, MAX_OUTSTANDING, "world_generation"),
        };
        json!({"transport_profile":name,"limits":{
            "max_request_bytes":crate::response::MAX_REQUEST_BYTES,
            "max_response_bytes":self.reply_limit(),"max_outstanding_bytes":outstanding,
            "max_process_response_handles":crate::response::MAX_RESPONSES,
            "max_retained_requests":crate::ledger::MAX_RETAINED_REQUESTS,
            "max_retained_bytes":self.retained_limit(),
            "retention_response_reservation_bytes":self.reply_limit()},
            "outstanding_scope":scope,"managed_integration":false,
            "callback_batch_attestation":false,"callback_records_limit_qualified":false,
            "target_bytes_share_legacy_pool":false})
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Owner {
    pub(crate) world: u64,
    pub(crate) profile: Profile,
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
    #[serde(rename = "transport.select")]
    Select { transport_profile: String },
    #[serde(rename = "transport.get")]
    Get {},
}
impl Request {
    pub(crate) fn is_read(&self) -> bool {
        matches!(self, Self::Get {})
    }
}

pub(crate) fn descriptor() -> Value {
    let mut descriptor = Profile::Target.effective();
    descriptor["capability"] = json!(PROFILE);
    descriptor["selection"] = json!("explicit_version_1_transport.select_on_untouched_world");
    descriptor["effective_query"] = json!("transport.get");
    descriptor["process_target_byte_ceiling"] = json!(MAX_REPLY * crate::response::MAX_RESPONSES);
    descriptor["serialization_boundary"] = json!("bounded_json_writer_after_value_construction");
    descriptor
}
