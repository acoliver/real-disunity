use std::io::{self, Write};

use serde_json::{Value, json};

use super::NativeTransport;
use crate::HostError;
use crate::compatibility::{CompatibilityHost, RESPONSE_LIMIT};

pub(crate) const PROFILE: &str = "transport_byte_limits_v1";
const CONSUMER: &str = "managed_transport_profile_consumer_v1";
const REQUEST_LIMIT: usize = 1 << 20;
const REPLY_LIMIT: usize = 8 << 20;

fn invalid(message: &str) -> HostError {
    HostError::Compatibility(format!("byte transport: {message}"))
}

pub(in crate::compatibility) fn capability(value: &Value, name: &str) -> Result<(), HostError> {
    if !value["capabilities"]
        .as_array()
        .is_some_and(|a| a.contains(&json!(name)))
    {
        return Err(invalid("required transport capability is unavailable"));
    }
    Ok(())
}

fn effective(value: &Value) -> Result<(), HostError> {
    let limits = &value["limits"];
    if value["transport_profile"] != PROFILE
        || limits["max_request_bytes"] != REQUEST_LIMIT
        || limits["max_response_bytes"] != REPLY_LIMIT
        || limits["max_outstanding_bytes"] != 16 << 20
        || value["outstanding_scope"] != "world_generation"
    {
        return Err(invalid(
            "native effective profile does not match 1/8/16 MiB",
        ));
    }
    Ok(())
}

impl NativeTransport {
    pub(in crate::compatibility) fn select_byte_profile(
        &self,
        world: u64,
    ) -> Result<(), HostError> {
        let descriptor = self.request(0, &json!({"version":1,"op":"describe"}))?;
        capability(&descriptor, PROFILE)?;
        if self
            .byte_worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&world)
        {
            return Err(invalid("world transport selection is frozen"));
        }
        let selected = self.request(
            world,
            &json!({"version":1,"op":"transport.select","transport_profile":PROFILE}),
        )?;
        // Selection has committed natively. Keep the stricter bound even if validation fails.
        self.byte_worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(world);
        effective(&selected)?;
        self.verify_byte_profile(world)
    }

    pub(in crate::compatibility) fn verify_byte_profile(
        &self,
        world: u64,
    ) -> Result<(), HostError> {
        effective(&self.request(world, &json!({"version":1,"op":"transport.get"}))?)
    }

    pub(super) fn response_limit(&self, world: u64) -> usize {
        if self
            .byte_worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&world)
        {
            REPLY_LIMIT
        } else {
            RESPONSE_LIMIT
        }
    }

    pub(super) fn encode(&self, world: u64, request: &Value) -> Result<Vec<u8>, HostError> {
        encode_bridge(request, self.response_limit(world))
    }
}

impl CompatibilityHost {
    /// Selects the bounded transport on an owned untouched world before opening a phase session.
    pub fn select_byte_profile(&mut self, world: u64) -> Result<(), HostError> {
        if !self.worlds.contains(&world) || self.strict.contains_key(&world) {
            return Err(invalid("requires an owned world before session.open"));
        }
        capability(&self.bridge.description()?, CONSUMER)?;
        self.native.select_byte_profile(world)
    }

    pub(in crate::compatibility) fn validate_byte_manifest(
        &self,
        world: u64,
        fixture: &Value,
    ) -> Result<(), HostError> {
        let requirement = &fixture["requirements"]["transport_profile"];
        let selected = self
            .native
            .byte_worlds
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&world);
        if !selected && requirement.is_null() {
            self.bridge.response_limit.set(RESPONSE_LIMIT);
            return Ok(());
        }
        if !selected || requirement != PROFILE {
            return Err(invalid(
                "manifest must match explicit host transport selection",
            ));
        }
        if fixture["requirements"]["execution_profile"] != super::super::phase::PROFILE
            || !self.strict.contains_key(&world)
        {
            return Err(invalid(
                "byte transport requires an opened strict phase session",
            ));
        }
        self.native.verify_byte_profile(world)?;
        self.bridge.response_limit.set(REPLY_LIMIT);
        Ok(())
    }
}

struct BoundedRequest(Vec<u8>);
impl Write for BoundedRequest {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > REQUEST_LIMIT.saturating_sub(self.0.len()) {
            return Err(io::Error::other("native request exceeds one MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn released<T>(
    primary: Result<T, HostError>,
    cleanup: Result<(), HostError>,
) -> Result<T, HostError> {
    match (primary, cleanup) {
        (Err(first), Err(cleanup)) => Err(HostError::Compatibility(format!(
            "{first}; response release: {cleanup}"
        ))),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

pub(in crate::compatibility) fn encode_bridge(
    request: &Value,
    reply_limit: usize,
) -> Result<Vec<u8>, HostError> {
    if reply_limit != REPLY_LIMIT {
        return serde_json::to_vec(request).map_err(|e| HostError::Request(e.to_string()));
    }
    let mut writer = BoundedRequest(Vec::new());
    serde_json::to_writer(&mut writer, request).map_err(|e| HostError::Request(e.to_string()))?;
    Ok(writer.0)
}
