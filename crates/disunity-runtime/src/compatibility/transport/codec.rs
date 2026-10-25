use crate::HostError;
use serde_json::{Value, json};
use std::io::{self, Write};

const REQUEST_LIMIT: usize = 1 << 20;

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

pub(in crate::compatibility) fn encode_bridge(request: &Value) -> Result<Vec<u8>, HostError> {
    let mut writer = BoundedRequest(Vec::new());
    serde_json::to_writer(&mut writer, request).map_err(|e| HostError::Request(e.to_string()))?;
    Ok(writer.0)
}

pub(in crate::compatibility) fn validate_limits(
    value: &Value,
    native: bool,
) -> Result<(), HostError> {
    capability(value, "byte_limits")?;
    for (key, expected) in [
        ("max_request_bytes", REQUEST_LIMIT),
        ("max_response_bytes", 8 << 20),
    ] {
        if value["limits"][key] != expected {
            return Err(invalid("provider byte limits must match 1/8 MiB"));
        }
    }
    if native
        && (value["limits"]["max_outstanding_bytes"] != 16 << 20
            || value["limits"]["max_responses"] != 1024)
    {
        return Err(invalid(
            "native owner quota and handle limit must match 16 MiB/1024",
        ));
    }
    Ok(())
}
