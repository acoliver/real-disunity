use serde_json::Value;

use crate::HostError;

/// One original managed exception. Missing native frame context stays unknown.
#[derive(Debug)]
pub struct CallbackFailure {
    pub sequence: u64,
    pub component: u64,
    pub script_type: String,
    pub callback: String,
    pub phase: String,
    pub frame: Option<u64>,
    pub exception_type: String,
    pub message: String,
    pub stack: String,
    pub native_code: Option<String>,
}

/// Read-only evidence page. Overflow remains a failing outcome until a new fixture.
#[derive(Debug)]
pub struct CallbackFailurePage {
    pub total: u64,
    pub retained: u64,
    pub dropped: u64,
    pub records: Vec<CallbackFailure>,
    pub next: u64,
    pub has_more: bool,
}

impl CallbackFailurePage {
    pub(super) fn parse(envelope: &Value) -> Result<Self, HostError> {
        if envelope["version"] != 1 || envelope["ok"] != true {
            return Err(invalid());
        }
        let data = &envelope["data"];
        let records = data["records"].as_array().ok_or_else(invalid)?;
        if records.len() > 32 {
            return Err(invalid());
        }
        let page = Self {
            total: number(data, "total")?,
            retained: number(data, "retained")?,
            dropped: number(data, "dropped")?,
            records: records
                .iter()
                .map(CallbackFailure::parse)
                .collect::<Result<_, _>>()?,
            next: number(data, "next")?,
            has_more: data["has_more"].as_bool().ok_or_else(invalid)?,
        };
        if page.retained > 4096
            || page.retained.checked_add(page.dropped) != Some(page.total)
            || page.next > page.retained
            || (page.total > 0 && page.records.is_empty() && page.next == 0)
        {
            return Err(invalid());
        }
        Ok(page)
    }
}

impl CallbackFailure {
    fn parse(value: &Value) -> Result<Self, HostError> {
        Ok(Self {
            sequence: number(value, "sequence")?,
            component: number(value, "component")?,
            script_type: text(value, "script_type", 256)?,
            callback: text(value, "callback", 64)?,
            phase: text(value, "phase", 64)?,
            frame: match &value["frame"] {
                Value::Null => None,
                frame => Some(frame.as_u64().ok_or_else(invalid)?),
            },
            exception_type: text(value, "exception_type", 256)?,
            message: text(value, "message", 4096)?,
            stack: text(value, "stack", 8192)?,
            native_code: match &value["native_code"] {
                Value::Null => None,
                _ => Some(text(value, "native_code", 256)?),
            },
        })
    }
}

fn number(value: &Value, key: &str) -> Result<u64, HostError> {
    value[key].as_u64().ok_or_else(invalid)
}

fn text(value: &Value, key: &str, limit: usize) -> Result<String, HostError> {
    let text = value[key].as_str().ok_or_else(invalid)?;
    if text.encode_utf16().count() > limit {
        return Err(invalid());
    }
    Ok(text.into())
}

fn invalid() -> HostError {
    HostError::Compatibility("invalid managed callback failure evidence".into())
}
