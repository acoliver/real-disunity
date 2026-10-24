use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, path::Path};

pub(crate) fn action(
    root: &Path,
    action: &Value,
    result: Option<&Result<(), String>>,
) -> Result<(), String> {
    let event = json!({
        "frame":action["frame"], "phase":action["phase"], "action":action["action"],
        "stage":if result.is_some() {"complete"} else {"begin"},
        "error":result.and_then(|result|result.as_ref().err()),
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("player-events.ndjson"))
        .map_err(|error| error.to_string())?;
    writeln!(file, "{event}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_actions_keep_a_completed_stage_with_the_real_error() {
        let output = tempfile::tempdir().unwrap();
        let stimulus = json!({"frame":17,"phase":"fixed","action":"advance"});
        action(output.path(), &stimulus, None).unwrap();
        action(
            output.path(),
            &stimulus,
            Some(&Err("native contact dispatch failed".into())),
        )
        .unwrap();
        let events = std::fs::read_to_string(output.path().join("player-events.ndjson")).unwrap();
        let events: Vec<Value> = events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["stage"], "begin");
        assert_eq!(events[1]["stage"], "complete");
        assert_eq!(events[1]["error"], "native contact dispatch failed");
        assert_eq!(events[1]["frame"], 17);
    }
}
