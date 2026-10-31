//! A player's legacy input axes, from the `InputManager` object of `globalgamemanagers`.
//!
//! A project lists its axes in `ProjectSettings/InputManager.asset` (see [`crate::input`]);
//! a built player keeps the same list in a binary object. Both plan as [`InputAxis`] in
//! manager order, because `Input.GetAxis` reads every entry with the requested name.

use std::path::Path;

use worldrunner_unity_serialized::{
    ClassId,
    managers::{InputAxisSettings, InputManager},
};

use crate::{
    PlayerError,
    input::{AxisKind, InputAxis},
    player_files::PlayerFiles,
};

/// The axes of the `InputManager` in `managers`, and a note for each axis whose device kind
/// the engine does not know, which is left out. A player without an input manager has no axes.
///
/// # Errors
/// Fails when `managers` cannot be read or its input manager does not decode.
pub(crate) fn plan_player_axes(
    files: &mut PlayerFiles,
    managers: &Path,
) -> Result<(Vec<InputAxis>, Vec<String>), PlayerError> {
    let open = files.open_path(managers)?;
    let Some(path_id) = open
        .file
        .objects()
        .iter()
        .find(|object| object.class_id == ClassId::INPUT_MANAGER)
        .map(|object| object.path_id)
    else {
        return Ok((Vec::new(), Vec::new()));
    };
    let manager: InputManager = open.decode(path_id)?;
    let mut axes = Vec::with_capacity(manager.axes.len());
    let mut notes = Vec::new();
    for settings in manager.axes {
        match axis(&settings) {
            Some(planned) => axes.push(planned),
            None => notes.push(format!(
                "input axis {} has the unknown device kind {}; it is not planned",
                settings.name, settings.kind
            )),
        }
    }
    Ok((axes, notes))
}

fn axis(settings: &InputAxisSettings) -> Option<InputAxis> {
    let kind = match settings.kind {
        0 => AxisKind::KeyOrMouseButton,
        1 => AxisKind::MouseMovement,
        2 => AxisKind::JoystickAxis,
        _ => return None,
    };
    Some(InputAxis {
        name: settings.name.clone(),
        kind,
        negative: settings.negative_button.clone(),
        positive: settings.positive_button.clone(),
        alt_negative: settings.alt_negative_button.clone(),
        alt_positive: settings.alt_positive_button.clone(),
        gravity: f64::from(settings.gravity),
        dead: f64::from(settings.dead),
        sensitivity: f64::from(settings.sensitivity),
        snap: settings.snap,
        invert: settings.invert,
    })
}
