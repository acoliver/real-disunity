//! Typed native controls attached to existing world object identities.
use super::{UiLayoutRules, group::groups_by_object};
use crate::ObjectHandle;
use std::collections::{BTreeMap, BTreeSet};
mod keyboard;
mod pointer;
mod slider;
mod style;
mod targets;
pub use slider::UiSlider;
pub use style::UiStyle;
use targets::Targets;
pub(super) use targets::contains;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiControlKind {
    Button,
    Slider(UiSlider),
    Toggle { on: bool },
    Choice { group: ObjectHandle, index: usize },
    ListItem { list: ObjectHandle, index: usize },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiControl {
    pub enabled: bool,
    pub kind: UiControlKind,
}
impl UiControl {
    pub fn slider(slider: UiSlider) -> Self {
        Self {
            enabled: true,
            kind: UiControlKind::Slider(slider),
        }
    }
    pub fn toggle(on: bool) -> Self {
        Self {
            enabled: true,
            kind: UiControlKind::Toggle { on },
        }
    }
    pub fn button() -> Self {
        Self {
            enabled: true,
            kind: UiControlKind::Button,
        }
    }
    pub fn choice(group: ObjectHandle, index: usize) -> Self {
        Self {
            enabled: true,
            kind: UiControlKind::Choice { group, index },
        }
    }
    pub fn list_item(list: ObjectHandle, index: usize) -> Self {
        Self {
            enabled: true,
            kind: UiControlKind::ListItem { list, index },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiAction {
    ValueChanged { control: ObjectHandle, value: f64 },
    Toggled { control: ObjectHandle, on: bool },
    Activated { object: ObjectHandle },
    ChoiceSelected { group: ObjectHandle, index: usize },
    Selected { list: ObjectHandle, index: usize },
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiResponse {
    /// Route the event to UI only when true, not also to scene interaction.
    pub consumed: bool,
    pub actions: Vec<UiAction>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiKey {
    Tab,
    Shift,
    Enter,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// A caller-owned interaction session over one world's object handles. Controls and modal roots
/// are explicit registrations; removing a registration disables its outstanding capture.
#[derive(Default)]
pub struct UiRouter {
    pub controls: BTreeMap<ObjectHandle, UiControl>,
    /// The highest sorted visible modal root restricts all interaction to its subtree.
    pub modals: BTreeSet<ObjectHandle>,
    pub layout: UiLayoutRules,
    pub styles: BTreeMap<ObjectHandle, UiStyle>,
    position: Option<[f64; 2]>,
    capture: Option<ObjectHandle>,
    primary_held: bool,
    focus: Option<ObjectHandle>,
    keys: BTreeSet<UiKey>,
    choices: BTreeMap<ObjectHandle, usize>,
    modal_focus: Vec<(ObjectHandle, Option<ObjectHandle>)>,
}
impl UiRouter {
    pub fn focused(&self) -> Option<ObjectHandle> {
        self.focus
    }
    pub fn selected(&self, group: ObjectHandle) -> Option<usize> {
        self.choices.get(&group).copied()
    }

    fn modal_changed(&mut self, modal: Option<ObjectHandle>) {
        if self.modal_focus.last().map(|(object, _)| *object) == modal {
            return;
        }
        if modal.is_none() || self.modal_focus.iter().any(|(o, _)| Some(*o) == modal) {
            while self.modal_focus.last().map(|(object, _)| *object) != modal {
                self.focus = self.modal_focus.pop().expect("tracked modal").1;
            }
        } else if let Some(object) = modal {
            self.modal_focus.push((object, self.focus));
            self.focus = None;
        }
        self.capture = None;
    }
    fn cancel(&mut self) {
        self.capture = None;
        self.primary_held = false;
        self.position = None;
        self.focus = None;
        self.keys.clear();
        self.modal_focus.clear();
    }
    fn activate(&mut self, object: ObjectHandle) -> Option<UiAction> {
        let kind = &mut self
            .controls
            .get_mut(&object)
            .expect("registered control")
            .kind;
        let action = match *kind {
            UiControlKind::Slider(_) => return None,
            UiControlKind::Toggle { on } => {
                *kind = UiControlKind::Toggle { on: !on };
                UiAction::Toggled {
                    control: object,
                    on: !on,
                }
            }
            UiControlKind::Button => UiAction::Activated { object },
            UiControlKind::Choice { group, index } => {
                self.choices.insert(group, index);
                UiAction::ChoiceSelected { group, index }
            }
            UiControlKind::ListItem { list, index } => {
                self.choices.insert(list, index);
                UiAction::Selected { list, index }
            }
        };
        Some(action)
    }
}
