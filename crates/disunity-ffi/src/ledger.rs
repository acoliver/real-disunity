use std::collections::BTreeMap;

pub(crate) const MAX_RETAINED_REQUESTS: usize = 1024;
pub(crate) const MAX_RETAINED_BYTES: usize = 32 << 20;
pub(crate) const MAX_REQUEST_ID_BYTES: usize = 128;

struct Entry {
    request: Vec<u8>,
    response: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct Ledger {
    entries: BTreeMap<String, Entry>,
    bytes: usize,
}
impl Ledger {
    pub(crate) fn replay(&self, id: &str, request: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let Some(entry) = self.entries.get(id) else {
            return Ok(None);
        };
        if entry.request != request {
            return Err("requestId reused with different request bytes".into());
        }
        Ok(Some(entry.response.clone()))
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(crate) fn reserve(
        &self,
        id: &str,
        request: &[u8],
        profile: crate::transport::Profile,
    ) -> Result<(), i32> {
        // A new identity must fit its worst-case semantic result before execution.
        let required = id.len() + request.len() + profile.reply_limit();
        if self.entries.len() >= MAX_RETAINED_REQUESTS
            || self.bytes + required > profile.retained_limit()
        {
            return Err(crate::result_code::BUDGET);
        }
        Ok(())
    }
    pub(crate) fn retain(&mut self, id: String, request: &[u8], response: &[u8]) {
        self.bytes += id.len() + request.len() + response.len();
        self.entries.insert(
            id,
            Entry {
                request: request.to_vec(),
                response: response.to_vec(),
            },
        );
    }
}
