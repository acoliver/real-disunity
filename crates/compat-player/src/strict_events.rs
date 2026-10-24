use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireInput {
    event_id: u64,
    timestamp: Value,
    source: String,
    kind: String,
    key: Option<String>,
    button: Option<String>,
    pressed: Option<bool>,
    axis: Option<String>,
    value: Option<f32>,
    focused: Option<bool>,
}

pub(crate) fn validate(value: &Value) -> Result<(), String> {
    let batch = value.as_array().ok_or("input_events requires an array")?;
    if batch.is_empty() || batch.len() > 4096 {
        return Err("input_events requires 1..4096 records".into());
    }
    let mut previous = (0, (0, 1));
    let mut source = None;
    for record in batch {
        let event: WireInput =
            serde_json::from_value(record.clone()).map_err(|error| error.to_string())?;
        let timestamp = crate::strict::ratio(&event.timestamp)?;
        if event.event_id <= previous.0 || before(timestamp, previous.1) {
            return Err("input event IDs must increase and timestamps must not decrease".into());
        }
        if !matches!(event.source.as_str(), "os" | "replay")
            || source
                .as_ref()
                .is_some_and(|source| source != &event.source)
        {
            return Err("input_events requires one supported source domain".into());
        }
        source = Some(event.source.clone());
        previous = (event.event_id, timestamp);
        validate_kind(event)?;
    }
    Ok(())
}

fn before(left: (u64, u64), right: (u64, u64)) -> bool {
    u128::from(left.0) * u128::from(right.1) < u128::from(right.0) * u128::from(left.1)
}

fn validate_kind(event: WireInput) -> Result<(), String> {
    match (
        event.kind.as_str(),
        event.key,
        event.button,
        event.pressed,
        event.axis,
        event.value,
        event.focused,
    ) {
        ("key", Some(key), None, Some(_), None, None, None)
        | ("button", None, Some(key), Some(_), None, None, None) => valid_name(&key),
        ("axis", None, None, None, Some(axis), Some(value), None) => {
            valid_name(&axis)?;
            if value.is_finite() && (-1.0..=1.0).contains(&value) {
                Ok(())
            } else {
                Err("normalized axis must be finite in -1..1".into())
            }
        }
        ("focus", None, None, None, None, None, Some(_)) => Ok(()),
        _ => Err("event fields do not match event kind".into()),
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err("invalid normalized input name".into());
    }
    Ok(())
}
