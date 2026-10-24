use super::Request;
use serde_json::Value;

pub(crate) struct Envelope {
    pub(crate) id: Option<String>,
    pub(crate) request: Result<Request, String>,
}
impl Envelope {
    pub(crate) fn parse(mut value: Value) -> Result<Self, String> {
        let map = value
            .as_object_mut()
            .ok_or("request must be a JSON object")?;
        let versioned = map.get("version").and_then(Value::as_u64) == Some(1);
        if let Some(version) = map.remove("version")
            && version.as_u64() != Some(1)
        {
            return Err("unsupported protocol version".into());
        }
        let id = map
            .remove("requestId")
            .map(|value| {
                let id = value.as_str().ok_or("requestId must be a string")?;
                if !versioned || id.is_empty() || id.len() > crate::ledger::MAX_REQUEST_ID_BYTES {
                    return Err("requestId requires version 1 and 1..128 UTF-8 bytes");
                }
                Ok(id.to_owned())
            })
            .transpose()?;
        if matches!(
            map.get("op").and_then(Value::as_str),
            Some("transport.select" | "transport.get")
        ) && (!versioned || id.is_some())
        {
            return Err("transport controls require explicit version 1 without requestId".into());
        }
        let request = if !versioned && map.get("op").and_then(Value::as_str) == Some("transaction")
        {
            Err("transaction requires explicit version 1".into())
        } else {
            serde_json::from_value::<Request>(value)
                .map_err(|error| error.to_string())
                .and_then(|mut r| {
                    if matches!(r, Request::Transport(_)) && !versioned {
                        return Err("transport controls require explicit version 1".into());
                    }
                    if id.is_some() && r.is_linear() {
                        return Err("strict controls reject requestId".into());
                    }
                    r.prepare()?;
                    Ok(r)
                })
        };
        Ok(Self { id, request })
    }
    pub(crate) fn is_unretained_read(&self) -> bool {
        self.id.is_none() && self.request.as_ref().is_ok_and(Request::is_read)
    }
    pub(crate) fn global_descriptor(&self) -> Option<Value> {
        self.id
            .is_none()
            .then(|| self.request.as_ref().ok()?.global_descriptor())
            .flatten()
    }
}
