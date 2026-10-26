//! The project's legacy input axes, from `ProjectSettings/InputManager.asset`.
//!
//! Entries keep their file order, because `Input.GetAxis` reads every entry with the requested
//! name and returns the one with the greatest magnitude. Button names stay Unity's text, such as
//! `left` or `left shift`; the runtime that owns the key codes maps them.

use std::path::Path;

use crate::{
    project::{ImportError, Project},
    yaml::{Mapping, Node, Scalar},
};

const INPUT_MANAGER: &str = "ProjectSettings/InputManager.asset";

/// The device an axis reads, Unity's `AxisType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisKind {
    KeyOrMouseButton,
    MouseMovement,
    JoystickAxis,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputAxis {
    pub name: String,
    pub kind: AxisKind,
    pub negative: String,
    pub positive: String,
    pub alt_negative: String,
    pub alt_positive: String,
    pub gravity: f64,
    pub dead: f64,
    pub sensitivity: f64,
    pub snap: bool,
    pub invert: bool,
}

/// The project's axes in file order. A project without an input manager file has none.
pub fn input_axes(project: &Project) -> Result<Vec<InputAxis>, ImportError> {
    let path = Path::new(INPUT_MANAGER);
    if !project.root().join(path).is_file() {
        return Ok(Vec::new());
    }
    let invalid = |message: String| ImportError::Invalid {
        path: path.to_owned(),
        message,
    };
    let documents = project.read_documents(path)?;
    let [document] = documents.as_slice() else {
        return Err(invalid("expected one InputManager document".into()));
    };
    let entries = document
        .body
        .get("m_Axes")
        .and_then(Node::as_sequence)
        .ok_or_else(|| invalid("has no m_Axes list".into()))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            entry
                .as_mapping()
                .ok_or_else(|| "is not a mapping".to_owned())
                .and_then(axis)
                .map_err(|message| invalid(format!("axis {index}: {message}")))
        })
        .collect()
}

fn axis(entry: &Mapping) -> Result<InputAxis, String> {
    let text = |key: &str| -> Result<String, String> {
        entry
            .get(key)
            .map_or(Some(""), |node| Some(node.as_scalar()?.text()))
            .map(str::to_owned)
            .ok_or(format!("invalid {key}"))
    };
    let number = |key: &str| -> Result<f64, String> {
        entry
            .get(key)
            .and_then(Node::as_scalar)
            .and_then(Scalar::as_f64)
            .filter(|value| value.is_finite())
            .ok_or(format!("invalid {key}"))
    };
    let flag = |key: &str| -> Result<bool, String> {
        entry
            .get(key)
            .and_then(Node::as_scalar)
            .and_then(Scalar::as_bool)
            .ok_or(format!("invalid {key}"))
    };
    let kind = match number("type")? {
        0.0 => AxisKind::KeyOrMouseButton,
        1.0 => AxisKind::MouseMovement,
        2.0 => AxisKind::JoystickAxis,
        _ => return Err("invalid type".into()),
    };
    let name = text("m_Name")?;
    if name.is_empty() {
        return Err("has no m_Name".into());
    }
    Ok(InputAxis {
        name,
        kind,
        negative: text("negativeButton")?,
        positive: text("positiveButton")?,
        alt_negative: text("altNegativeButton")?,
        alt_positive: text("altPositiveButton")?,
        gravity: number("gravity")?,
        dead: number("dead")?,
        sensitivity: number("sensitivity")?,
        snap: flag("snap")?,
        invert: flag("invert")?,
    })
}
