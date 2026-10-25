use super::{NativeTransport, check_native, validate_response};
use crate::{HostError, compatibility::HostIdleMutationError};
use serde_json::Value;

fn unknown(primary: HostError) -> HostIdleMutationError {
    HostIdleMutationError::OutcomeUnknown {
        primary,
        cleanup: Vec::new(),
    }
}
impl NativeTransport {
    pub(in crate::compatibility) fn request_idle_mutation(
        &self,
        world: u64,
        request: &Value,
    ) -> Result<Value, HostIdleMutationError> {
        use HostIdleMutationError::Rejected;
        let bytes = super::codec::encode_bridge(request).map_err(Rejected)?;
        if bytes.len() > 1024 * 1024 {
            return Err(Rejected(HostError::Request(
                "native request exceeds one MiB".into(),
            )));
        }
        let mut response = 0;
        // SAFETY: the bounded buffer and writable output live through this synchronous call.
        let submitted = check_native(unsafe {
            (self.request)(world, bytes.as_ptr(), bytes.len(), &mut response)
        });
        if let Err(error) = submitted {
            return Err(unknown(error));
        }
        let copied = self.read_response(response, super::super::RESPONSE_LIMIT);
        // SAFETY: this response belongs to this submission; no mutation is repeated.
        let released = check_native(unsafe { (self.free)(response) });
        self.classify_idle_response(world, response, request, copied, released)
    }
    fn classify_idle_response(
        &self,
        world: u64,
        handle: u64,
        request: &Value,
        copied: Result<Vec<u8>, HostError>,
        released: Result<(), HostError>,
    ) -> Result<Value, HostIdleMutationError> {
        if released.is_err() {
            self.idle_responses
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(handle, world);
        }
        let decoded = copied.and_then(|bytes| {
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| HostError::Request("host mutation response is malformed".into()))
        });
        let value = match (decoded, released) {
            (Err(primary), Err(release)) => {
                return Err(HostIdleMutationError::OutcomeUnknown {
                    primary,
                    cleanup: vec![release],
                });
            }
            (Err(primary), _) | (_, Err(primary)) => return Err(unknown(primary)),
            (Ok(value), Ok(())) => value,
        };
        if value["version"] != 1 || !value["ok"].is_boolean() {
            return Err(unknown(HostError::Request(
                "host mutation response envelope is malformed".into(),
            )));
        }
        if value["ok"] == false {
            return Err(HostIdleMutationError::Rejected(HostError::Request(
                "native host mutation rejected".into(),
            )));
        }
        validate_response(request, value).map_err(unknown)
    }
}

impl NativeTransport {
    pub(in crate::compatibility) fn release_idle_responses(&self, world: u64) -> Vec<HostError> {
        let mut retained = self
            .idle_responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let handles = retained
            .iter()
            .filter_map(|(handle, owner)| (*owner == world).then_some(*handle))
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for handle in handles {
            // SAFETY: only this recorded generation is released, at most once per
            // cleanup call. Native INVALID_HANDLE (1) proves this generation absent.
            let result = unsafe { (self.free)(handle) };
            match result.code {
                0 => {
                    retained.remove(&handle);
                }
                1 => {
                    retained.remove(&handle);
                    errors.push(HostError::Compatibility(format!("response {handle}: generation already absent after uncertain release (native INVALID_HANDLE)")));
                }
                code => errors.push(HostError::Abi(code)),
            }
        }
        errors
    }
    pub(in crate::compatibility) fn idle_response_count(&self) -> usize {
        self.idle_responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}
