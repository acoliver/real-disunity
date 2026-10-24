use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use libloading::Library;
use serde_json::Value;

use super::check_native;
use crate::{DisunityResult, HostError};

type Request = unsafe extern "C" fn(u64, *const u8, usize, *mut u64) -> DisunityResult;
type CopyResponse = unsafe extern "C" fn(u64, *mut u8, usize, *mut usize) -> DisunityResult;
type FreeResponse = unsafe extern "C" fn(u64) -> DisunityResult;

pub(super) mod byte_profile;
mod idle_mutation;

pub(super) struct NativeTransport {
    byte_worlds: Mutex<BTreeSet<u64>>,
    idle_responses: Mutex<BTreeMap<u64, u64>>,
    create: unsafe extern "C" fn(*mut u64) -> DisunityResult,
    destroy: unsafe extern "C" fn(u64) -> DisunityResult,
    request: Request,
    copy: CopyResponse,
    free: FreeResponse,
    statistics: unsafe extern "C" fn(*mut u64, *mut u64) -> DisunityResult,
}

#[cfg(test)]
mod byte_bridge_tests;
#[cfg(test)]
mod byte_tests;

impl NativeTransport {
    pub(super) fn bind(library: &Library) -> Result<Self, HostError> {
        macro_rules! bind {
            ($name:literal) => {
                // SAFETY: callers retain the explicitly selected image for every symbol's lifetime.
                unsafe {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|_| HostError::MissingSymbol($name.into()))?
                }
            };
        }
        let version: unsafe extern "C" fn(*mut u32) -> DisunityResult =
            bind!("disunity_abi_version");
        let mut actual = 0;
        // SAFETY: the version output is writable and the explicitly selected image remains mapped.
        check_native(unsafe { version(&mut actual) })?;
        if actual != 2 {
            return Err(HostError::Request(format!(
                "compatibility requires native ABI 2, got {actual}"
            )));
        }
        Ok(Self {
            byte_worlds: Mutex::default(),
            idle_responses: Mutex::default(),
            create: bind!("disunity_world_create"),
            destroy: bind!("disunity_world_destroy"),
            request: bind!("disunity_world_request"),
            copy: bind!("disunity_response_copy"),
            free: bind!("disunity_response_free"),
            statistics: bind!("disunity_store_statistics"),
        })
    }

    pub(super) fn create_world(&self) -> Result<u64, HostError> {
        let mut world = 0;
        // SAFETY: the output points to a writable native handle.
        check_native(unsafe { (self.create)(&mut world) })?;
        Ok(world)
    }

    pub(super) fn destroy_world(&self, world: u64) -> Result<(), HostError> {
        if self
            .idle_responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .any(|owner| *owner == world)
        {
            return Err(HostError::Compatibility(
                "cleanup_incomplete: host mutation response release remains unresolved".into(),
            ));
        }
        // SAFETY: the ABI validates this generation-checked handle.
        check_native(unsafe { (self.destroy)(world) })?;
        self.byte_worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&world);
        Ok(())
    }

    pub(super) fn request(&self, world: u64, request: &Value) -> Result<Value, HostError> {
        let bytes = self.request_bytes(world, request)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| HostError::Request(error.to_string()))?;
        validate_response(request, value)
    }

    // Contacts must reach CLR without the default JSON parser rounding native geometry.
    pub(super) fn request_text(&self, world: u64, request: &Value) -> Result<String, HostError> {
        let bytes = self.request_bytes(world, request)?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|error| HostError::Request(error.to_string()))?;
        validate_response(request, value)?;
        String::from_utf8(bytes).map_err(|error| HostError::Request(error.to_string()))
    }

    fn request_bytes(&self, world: u64, request: &Value) -> Result<Vec<u8>, HostError> {
        let bytes = self.encode(world, request)?;
        if bytes.len() > 1024 * 1024 {
            return Err(HostError::Request("native request exceeds one MiB".into()));
        }
        let mut response = 0;
        // SAFETY: the request is borrowed for this call and the output handle is writable.
        check_native(unsafe { (self.request)(world, bytes.as_ptr(), bytes.len(), &mut response) })?;
        let result = self.read_response(response, self.response_limit(world));
        // SAFETY: the request returned this response handle; release it on all copy/parse paths.
        let released = check_native(unsafe { (self.free)(response) });
        byte_profile::released(result, released)
    }

    fn read_response(&self, handle: u64, limit: usize) -> Result<Vec<u8>, HostError> {
        let mut required = 0;
        // SAFETY: a null-buffer probe returns the required length without consuming the result.
        let probe = unsafe { (self.copy)(handle, std::ptr::null_mut(), 0, &mut required) };
        if probe.code != 2 {
            check_native(probe)?;
        }
        if required == 0 {
            check_native(probe)?;
            return Err(HostError::Request("native response was empty".into()));
        }
        if required > limit {
            return Err(HostError::ResponseBudget(limit));
        }
        let mut bytes = vec![0; required];
        // SAFETY: the buffer has the exact capacity reported by the retained response.
        check_native(unsafe {
            (self.copy)(handle, bytes.as_mut_ptr(), bytes.len(), &mut required)
        })?;
        if required != bytes.len() {
            return Err(HostError::Request(
                "native response length changed during copy".into(),
            ));
        }
        Ok(bytes)
    }

    pub(super) fn statistics(&self) -> Result<(u64, u64), HostError> {
        let (mut live, mut allocated) = (0, 0);
        // SAFETY: both output pointers are writable and live for the call.
        check_native(unsafe { (self.statistics)(&mut live, &mut allocated) })?;
        Ok((live, allocated))
    }
}

fn validate_response(request: &Value, value: Value) -> Result<Value, HostError> {
    if value["version"].as_u64() != Some(1) {
        return Err(HostError::Request(
            "native response has an incompatible protocol version".into(),
        ));
    }
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(HostError::Request(format!(
            "{}: {}",
            request["op"], value["error"]
        )));
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| HostError::Request("native response omitted its data field".into()))
}
#[cfg(test)]
mod idle_mutation_tests;

#[cfg(test)]
impl NativeTransport {
    pub(super) fn copy_for_cleanup_test(&self) -> CopyResponse {
        self.copy
    }
    pub(super) fn set_copy_for_cleanup_test(&mut self, copy: CopyResponse) {
        self.copy = copy;
    }
    pub(super) fn probe_response_for_cleanup_test(&self, handle: u64) -> i32 {
        let mut size = 0;
        // SAFETY: null-buffer probe observes the exact generation without freeing it.
        unsafe { (self.copy)(handle, std::ptr::null_mut(), 0, &mut size).code }
    }
    pub(super) fn free_for_cleanup_test(&self) -> FreeResponse {
        self.free
    }
    pub(super) fn set_free_for_cleanup_test(&mut self, free: FreeResponse) {
        self.free = free;
    }
}

#[cfg(test)]
pub(super) type CleanupFree = FreeResponse;

#[cfg(test)]
pub(super) type CleanupCopy = CopyResponse;
