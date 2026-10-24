use crate::ratio::Ratio;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    Os,
    Replay,
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "WireEvent")]
pub(crate) struct Event {
    pub event_id: u64,
    pub timestamp: Ratio,
    pub source: Source,
    pub action: Action,
}
#[derive(Clone)]
pub(crate) enum Action {
    Key(String, bool),
    Axis(String, f32),
    Focus(bool),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEvent {
    event_id: u64,
    timestamp: Ratio,
    source: Source,
    kind: String,
    key: Option<String>,
    button: Option<String>,
    pressed: Option<bool>,
    axis: Option<String>,
    value: Option<f32>,
    focused: Option<bool>,
}
impl TryFrom<WireEvent> for Event {
    type Error = String;
    fn try_from(w: WireEvent) -> Result<Self, String> {
        let timestamp = w.timestamp.checked()?;
        let action = match (
            w.kind.as_str(),
            w.key,
            w.button,
            w.pressed,
            w.axis,
            w.value,
            w.focused,
        ) {
            ("key", Some(key), None, Some(pressed), None, None, None)
            | ("button", None, Some(key), Some(pressed), None, None, None) => {
                valid_name(&key)?;
                Action::Key(key, pressed)
            }
            ("axis", None, None, None, Some(axis), Some(value), None) => {
                valid_name(&axis)?;
                if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
                    return Err("normalized axis must be finite in -1..1".into());
                }
                Action::Axis(axis, value)
            }
            ("focus", None, None, None, None, None, Some(focused)) => Action::Focus(focused),
            _ => return Err("event fields do not match event kind".into()),
        };
        Ok(Self {
            event_id: w.event_id,
            timestamp,
            source: w.source,
            action,
        })
    }
}
fn valid_name(name: &str) -> Result<(), String> {
    if !super::axes::valid_name(name) {
        return Err("invalid normalized input name".into());
    }
    Ok(())
}
