#[cfg(test)]
#[path = "resource_path_tests.rs"]
mod resource_path_tests;
use base64::{Engine, engine::general_purpose::STANDARD};
use compat_kit::{RunRequest, files};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub(crate) fn commands(root: &Path, request: &RunRequest) -> Result<Vec<Value>, String> {
    let mut commands = Vec::new();
    if let Some(fonts) = request.metadata["fonts"].as_object() {
        for (name, path) in fonts {
            let path = root.join(path.as_str().ok_or("font resource path must be a string")?);
            let bytes = read_resource(root, &path, 780_000)
                .map_err(|error| format!("font {}: {error}", path.display()))?;
            if bytes.is_empty() {
                return Err("font resource is empty".into());
            }
            commands.push(
                json!({"op":"font.register","name":name,"data_base64":STANDARD.encode(bytes)}),
            );
        }
    }
    let path = request
        .source_root
        .join("ProjectSettings/InputManager.asset");
    if path.symlink_metadata().is_ok() {
        let bytes = read_resource(&request.source_root, &path, 1_048_576)?;
        let text = String::from_utf8(bytes).map_err(files::io)?;
        commands.push(axis_command(&text)?);
    }
    Ok(commands)
}

fn read_resource(base: &Path, path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let canonical = base.canonicalize().map_err(files::io)?;
    if canonical != base {
        return Err("resource root must be canonical and free of symlinks".into());
    }
    let relative = path
        .strip_prefix(base)
        .map_err(|_| "resource path escapes authorized root")?;
    let path = files::safe_file(base, relative)?;
    let file = fs::File::open(path).map_err(files::io)?;
    let metadata = file.metadata().map_err(files::io)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("resource exceeds admission byte budget".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(files::io)?;
    if u64::try_from(bytes.len()).map_err(files::io)? > limit {
        return Err("resource exceeds admission byte budget".into());
    }
    Ok(bytes)
}

fn axis_command(text: &str) -> Result<Value, String> {
    let docs = worldrunner::compat_content::yaml::parse_unity_yaml(text, 16)?;
    let manager = docs
        .iter()
        .find(|doc| doc.kind == "InputManager")
        .ok_or("input settings have no InputManager document")?;
    let mut axes: BTreeMap<String, Value> = BTreeMap::new();
    for axis in manager.fields["m_Axes"]
        .as_array()
        .ok_or("input settings have no axes")?
    {
        if axis["type"].as_u64() != Some(0) {
            continue;
        }
        let name = axis["m_Name"].as_str().ok_or("input axis has no name")?;
        let entry=axes.entry(name.into()).or_insert_with(||json!({
            "negative_keys":[],"positive_keys":[],"sensitivity":axis["sensitivity"],
            "gravity":axis["gravity"],"dead":axis["dead"],"snap":axis["snap"]==1,"invert":axis["invert"]==1
        }));
        bindings(
            entry,
            "negative_keys",
            axis,
            ["negativeButton", "altNegativeButton"],
        )?;
        bindings(
            entry,
            "positive_keys",
            axis,
            ["positiveButton", "altPositiveButton"],
        )?;
    }
    Ok(json!({"op":"input.configure_axes","axes":axes}))
}
fn bindings(entry: &mut Value, field: &str, axis: &Value, source: [&str; 2]) -> Result<(), String> {
    let keys = entry[field]
        .as_array_mut()
        .ok_or("axis key collection is malformed")?;
    for key in source.into_iter().filter_map(|field| axis[field].as_str()) {
        if key.is_empty() {
            continue;
        }
        let key = json!(key_code(key));
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(())
}
pub(crate) fn key_code(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "left" | "arrowleft" => "LeftArrow".into(),
        "right" | "arrowright" => "RightArrow".into(),
        "up" | "arrowup" => "UpArrow".into(),
        "down" | "arrowdown" => "DownArrow".into(),
        "enter" | "return" => "Return".into(),
        "space" => "Space".into(),
        "escape" => "Escape".into(),
        "left ctrl" => "LeftControl".into(),
        "right ctrl" => "RightControl".into(),
        value if value.len() == 1 => value.to_uppercase(),
        value => value
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_axis_duplicates_retain_all_keys_without_a_managed_input_world() {
        let pong =
            worldrunner_original_games::checkout(worldrunner_original_games::Game::Pong).unwrap();
        let text = fs::read_to_string(pong.join("ProjectSettings/InputManager.asset")).unwrap();
        let command = axis_command(&text).unwrap();
        assert_eq!(command["axes"]["Vertical"]["positive_keys"], json!(["W"]));
        assert_eq!(
            command["axes"]["AlternateVertical"]["negative_keys"],
            json!(["DownArrow"])
        );
        assert!(
            command["axes"]["Submit"]["positive_keys"]
                .as_array()
                .unwrap()
                .contains(&json!("Return"))
        );
        assert_eq!(key_code("a"), "A");
    }
}
