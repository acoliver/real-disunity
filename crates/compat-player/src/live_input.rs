use serde_json::{Value, json};
use std::collections::BTreeSet;
use worldrunner::scene::viewer::LiveInputEvent;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Reload,
    Capture,
    Stop,
}

pub(crate) struct Ingress {
    pub changed: bool,
    pub commands: Vec<Command>,
}

pub(crate) struct Input {
    os: BTreeSet<String>,
    script: BTreeSet<String>,
    controls: BTreeSet<String>,
    axes: Value,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            os: BTreeSet::new(),
            script: BTreeSet::new(),
            controls: BTreeSet::new(),
            axes: json!({}),
        }
    }
}

impl Input {
    pub fn ingress(&mut self, events: &[LiveInputEvent], interactive: bool) -> Ingress {
        let mut result = Ingress {
            changed: false,
            commands: Vec::new(),
        };
        for event in events {
            match event {
                LiveInputEvent::Key {
                    key,
                    pressed,
                    repeat: false,
                } => {
                    if interactive && self.control(key, *pressed, &mut result.commands) {
                        continue;
                    }
                    let key = crate::resources::key_code(key);
                    result.changed |= if *pressed {
                        self.os.insert(key)
                    } else {
                        self.os.remove(&key)
                    };
                }
                LiveInputEvent::Focused(false) => {
                    result.changed |= !self.os.is_empty();
                    self.os.clear();
                    self.controls.clear();
                }
                LiveInputEvent::CloseRequested => result.commands.push(Command::Stop),
                LiveInputEvent::CaptureRequested(_) if interactive => {
                    result.commands.push(Command::Capture);
                }
                _ => {}
            }
        }
        result
    }

    fn control(&mut self, key: &str, pressed: bool, commands: &mut Vec<Command>) -> bool {
        let command = match key.to_lowercase().as_str() {
            "f5" => Command::Reload,
            "f12" => Command::Capture,
            "escape" => Command::Stop,
            _ => return false,
        };
        let key = key.to_lowercase();
        if pressed && self.controls.insert(key.clone()) {
            commands.push(command);
        } else if !pressed {
            self.controls.remove(&key);
        }
        true
    }

    pub fn script(&mut self, action: &Value) -> Result<(), String> {
        self.script =
            serde_json::from_value(action["keys"].clone()).map_err(|error| error.to_string())?;
        self.axes = action.get("axes").cloned().unwrap_or_else(|| json!({}));
        Ok(())
    }

    pub fn state(&self) -> (Vec<String>, Value) {
        (
            self.script.union(&self.os).cloned().collect(),
            self.axes.clone(),
        )
    }

    pub fn clear(&mut self) {
        self.os.clear();
        self.script.clear();
        self.axes = json!({});
    }
}

#[cfg(test)]
#[path = "live_input_tests.rs"]
mod tests;
