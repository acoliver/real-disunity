//! Typed native controls attached to existing world object identities.
use super::{UiLayout, UiLayoutRules, group::groups_by_object};
use crate::{ObjectHandle, PointerButton, PointerEvent, World, WorldError};
use std::collections::{BTreeMap, BTreeSet};
mod keyboard;
mod style;
mod targets;
pub use style::UiStyle;
use targets::Targets;
pub(super) use targets::contains;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiControlKind {
    Button,
    Choice { group: ObjectHandle, index: usize },
    ListItem { list: ObjectHandle, index: usize },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiControl {
    pub enabled: bool,
    pub kind: UiControlKind,
}
impl UiControl {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Activated { object: ObjectHandle },
    ChoiceSelected { group: ObjectHandle, index: usize },
    Selected { list: ObjectHandle, index: usize },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

    /// Route observations against a freshly resolved layout. Host positions stay in physical
    /// pixels; the top-left to lower-left conversion occurs in hit testing.
    pub fn pointer(
        &mut self,
        world: &World,
        layout: &UiLayout,
        event: PointerEvent,
    ) -> Result<UiResponse, WorldError> {
        if !event.valid() {
            return Err(WorldError::InvalidInput);
        }
        if let PointerEvent::Moved(position) = event {
            self.position = Some(position);
        }
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        self.modal_changed(targets.modal);
        self.focus = self.focus.filter(|o| targets.interactable(*o));
        let hit = self.position.and_then(|position| targets.hit(position));
        let mut response = UiResponse {
            consumed: self.capture.is_some() || hit.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        match event {
            PointerEvent::Cancel => self.cancel(),
            PointerEvent::Wheel(wheel) => {
                if let Some(position) = self.position {
                    response.consumed |=
                        self.layout
                            .wheel(world, layout, position, wheel, targets.modal)?;
                }
            }
            PointerEvent::Button {
                button: PointerButton::Left,
                pressed,
            } => {
                self.primary_button(&targets, hit, pressed, &mut response);
            }
            _ => {}
        }
        Ok(response)
    }
    fn primary_button(
        &mut self,
        targets: &Targets<'_>,
        hit: Option<ObjectHandle>,
        pressed: bool,
        response: &mut UiResponse,
    ) {
        if pressed {
            if !self.primary_held {
                self.capture = hit.filter(|object| targets.interactable(*object));
                self.focus = self.capture;
            }
            self.primary_held = true;
        } else {
            let capture = self.capture.take();
            self.primary_held = false;
            if let Some(object) = capture.filter(|o| Some(*o) == hit && targets.interactable(*o)) {
                response.actions.push(self.activate(object));
            }
        }
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
    fn activate(&mut self, object: ObjectHandle) -> UiAction {
        match self.controls[&object].kind {
            UiControlKind::Button => UiAction::Activated { object },
            UiControlKind::Choice { group, index } => {
                self.choices.insert(group, index);
                UiAction::ChoiceSelected { group, index }
            }
            UiControlKind::ListItem { list, index } => {
                self.choices.insert(list, index);
                UiAction::Selected { list, index }
            }
        }
    }
}
