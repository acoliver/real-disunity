use crate::playback::Playback;
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, path::Path};

pub(crate) fn journal(root: &Path, event: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("interactive-events.ndjson"))
        .map_err(|error| error.to_string())?;
    writeln!(file, "{event}").map_err(|error| error.to_string())
}

pub(crate) fn capture(playback: &mut Playback<'_>, frame: u64) -> Result<(), String> {
    for selector in ["world.snapshot", "managed.snapshot"] {
        playback.action(
            &json!({"frame":frame,"phase":"render","action":"observe","selector":selector}),
        )?;
    }
    playback.action(&json!({"frame":frame,"phase":"render","action":"capture",
        "path":format!("captures/interactive-{frame:06}.png")}))
}
