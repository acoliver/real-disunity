//! Platform-free observations of the router's current layout and interaction state.
use super::TargetId;
use crate::ObjectHandle;
use crate::ui::UiLinkId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiNodeId {
    Window,
    Object(ObjectHandle),
    Link {
        object: ObjectHandle,
        link: UiLinkId,
    },
}
impl From<TargetId> for UiNodeId {
    fn from(target: TargetId) -> Self {
        match target.link {
            Some(link) => Self::Link {
                object: target.object,
                link,
            },
            None => Self::Object(target.object),
        }
    }
}
impl UiNodeId {
    pub(super) fn target(self) -> Option<TargetId> {
        match self {
            Self::Window => None,
            Self::Object(object) => Some(object.into()),
            Self::Link { object, link } => Some(TargetId {
                object,
                link: Some(link),
            }),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiRole {
    Window,
    Group,
    Button,
    Checkbox,
    Slider,
    List,
    ListItem,
    Dialog,
    Text,
    Link,
    Status,
}
/// Optional accessible name. Text components supply names automatically. Live names announce
/// when their text changes; ordinary geometry, focus and value updates do not announce them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiSemantics {
    pub label: Option<String>,
    pub live: bool,
}
impl UiSemantics {
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            live: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiSemanticValue {
    None,
    Toggled(bool),
    Selected(bool),
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: Option<f64>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiAccessAction {
    Focus,
    Press,
    Toggle,
    Increment,
    Decrement,
    Select,
    SetValue(f64),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiAccessRequest {
    pub target: UiNodeId,
    pub action: UiAccessAction,
}
#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticNode {
    pub id: UiNodeId,
    pub role: UiRole,
    pub label: String,
    pub value: UiSemanticValue,
    /// Axis-aligned [left, top, right, bottom] in physical client pixels. Affine screen
    /// corners supply these bounds; the host must not apply display scale again.
    pub bounds: [f64; 4],
    pub children: Vec<UiNodeId>,
    pub actions: Vec<UiAccessAction>,
    pub disabled: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticTree {
    pub nodes: BTreeMap<UiNodeId, UiSemanticNode>,
    pub focus: UiNodeId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAnnouncement {
    pub node: UiNodeId,
    pub text: String,
}
/// Changes since this router's preceding `semantic_update`. Apply removals then changed
/// nodes; focus is always the current focus. The first update contains the complete tree.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticUpdate {
    pub changed: Vec<UiSemanticNode>,
    pub removed: Vec<UiNodeId>,
    pub focus: UiNodeId,
    pub focus_changed: bool,
    pub announcements: Vec<UiAnnouncement>,
}
impl UiSemanticUpdate {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
            && self.removed.is_empty()
            && !self.focus_changed
            && self.announcements.is_empty()
    }
}
impl UiSemanticTree {
    pub(super) fn changes(&self, previous: Option<&Self>) -> UiSemanticUpdate {
        let mut changed = Vec::new();
        for (id, node) in &self.nodes {
            let old = previous.and_then(|tree| tree.nodes.get(id));
            if old != Some(node) {
                changed.push(node.clone());
            }
        }
        let removed = previous
            .into_iter()
            .flat_map(|tree| tree.nodes.keys())
            .filter(|id| !self.nodes.contains_key(id))
            .copied()
            .collect();
        UiSemanticUpdate {
            changed,
            removed,
            focus: self.focus,
            focus_changed: previous.is_none_or(|tree| tree.focus != self.focus),
            announcements: Vec::new(),
        }
    }
}
