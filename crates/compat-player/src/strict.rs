use compat_kit::RunRequest;
use serde_json::Value;
use std::time::Duration;

pub(crate) const PROFILE: &str = "native_phase_context_v1";
pub(crate) const CHUNK_NS: u64 = 16_666_666;
pub(crate) const MAX_CHUNKS: u64 = 15;
pub(crate) const CAP_NS: u64 = CHUNK_NS * MAX_CHUNKS;

pub(crate) fn selected(metadata: &Value) -> Result<bool, String> {
    match metadata.get("execution_profile") {
        None => Ok(false),
        Some(Value::String(profile)) if profile == PROFILE => Ok(true),
        Some(_) => Err("unsupported explicit execution_profile".into()),
    }
}

pub(crate) const STARTUP: &str = "ready_prewarmed_v1";

/// Explicit opt-in presentation startup. Only strict continuous desktop runs may select it.
pub(crate) fn startup(metadata: &Value, strict_interactive: bool) -> Result<bool, String> {
    match metadata.get("presentation_startup") {
        None => Ok(false),
        Some(Value::String(value)) if value == STARTUP && strict_interactive => Ok(true),
        Some(Value::String(value)) if value == STARTUP => {
            Err("presentation_startup requires a strict interactive desktop run".into())
        }
        Some(_) => Err("unsupported presentation_startup".into()),
    }
}

pub(crate) fn admit(request: &RunRequest) -> Result<bool, String> {
    let strict = selected(&request.metadata)?;
    for action in &request.timeline {
        action_mode(strict, action["action"].as_str().unwrap_or(""))?;
        match action["action"].as_str() {
            Some("advance_rational") => {
                ratio(&action["dt"])?;
            }
            Some("input_events") => {
                events(&action["events"])?;
            }
            _ => {}
        }
    }
    Ok(strict)
}

pub(crate) fn action_mode(strict: bool, action: &str) -> Result<(), String> {
    if (strict && matches!(action, "advance" | "input"))
        || (!strict && matches!(action, "advance_rational" | "input_events"))
    {
        return Err(format!(
            "timeline action {action} conflicts with execution_profile"
        ));
    }
    Ok(())
}

pub(crate) fn ratio(value: &Value) -> Result<(u64, u64), String> {
    let fields = value
        .as_object()
        .ok_or("rational time requires num and den")?;
    let num = value["num"]
        .as_u64()
        .ok_or("rational num must be an unsigned integer")?;
    let den = value["den"]
        .as_u64()
        .ok_or("rational den must be an unsigned integer")?;
    if fields.len() != 2 || num > 9_007_199_254_740_991 || den == 0 || den > 1_000_000_000_000 {
        return Err("rational component outside supported bounds".into());
    }
    Ok((num, den))
}

pub(crate) fn steps(duration: Duration) -> Result<Vec<Duration>, String> {
    let nanos = duration.as_nanos();
    if nanos > u128::from(CAP_NS) {
        return Err("strict catch-up exceeds 15 exact 16666666ns chunks".into());
    }
    let mut remaining = u64::try_from(nanos).map_err(|error| error.to_string())?;
    let mut chunks = Vec::new();
    while remaining > 0 {
        let step = remaining.min(CHUNK_NS);
        chunks.push(Duration::from_nanos(step));
        remaining -= step;
    }
    Ok(chunks)
}

pub(crate) fn events(value: &Value) -> Result<Value, String> {
    crate::strict_events::validate(value)?;
    Ok(value.clone())
}

#[cfg(test)]
#[path = "strict_tests.rs"]
mod tests;
