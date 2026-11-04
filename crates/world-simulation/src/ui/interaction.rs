//! Typed native controls attached to existing world object identities.
use super::{UiLayout, group::groups_by_object};
use crate::{ObjectHandle, PointerButton, PointerEvent, World, WorldError};
use std::collections::{BTreeMap, BTreeSet};
mod targets;
use targets::Targets;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiControlKind {
    Button,
    Choice { group: ObjectHandle, index: usize },
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Activated { object: ObjectHandle },
    ChoiceSelected { group: ObjectHandle, index: usize },
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
}

/// A caller-owned interaction session over one world's object handles. Controls and modal roots
/// are explicit registrations; removing a registration disables its outstanding capture.
#[derive(Default)]
pub struct UiRouter {
    pub controls: BTreeMap<ObjectHandle, UiControl>,
    /// The highest sorted visible modal root restricts all interaction to its subtree.
    pub modals: BTreeSet<ObjectHandle>,
    position: Option<[f64; 2]>,
    capture: Option<ObjectHandle>,
    primary_held: bool,
    focus: Option<ObjectHandle>,
    keys: BTreeSet<UiKey>,
    choices: BTreeMap<ObjectHandle, usize>,
}
impl UiRouter {
    pub fn focused(&self) -> Option<ObjectHandle> {
        self.focus
    }
    pub fn selected(&self, group: ObjectHandle) -> Option<usize> {
        self.choices.get(&group).copied()
    }

    /// Route ordered observations against a freshly resolved layout of `world`. Host positions
    /// stay in physical pixels; the top-left to lower-left conversion occurs in hit testing.
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
        let hit = self.position.and_then(|position| targets.hit(position));
        let mut response = UiResponse {
            consumed: self.capture.is_some() || hit.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        match event {
            PointerEvent::Cancel => self.cancel(),
            PointerEvent::Button {
                button: PointerButton::Left,
                pressed: true,
            } => {
                if !self.primary_held {
                    self.capture = hit.filter(|object| targets.interactable(*object));
                    self.focus = self.capture;
                }
                self.primary_held = true;
            }
            PointerEvent::Button {
                button: PointerButton::Left,
                pressed: false,
            } => {
                let capture = self.capture.take();
                self.primary_held = false;
                if let Some(object) =
                    capture.filter(|o| Some(*o) == hit && targets.interactable(*o))
                {
                    response.actions.push(self.activate(object));
                }
            }
            _ => {}
        }
        Ok(response)
    }

    /// Key identities are mapped by the caller from its input bindings. Activation occurs on the
    /// first key-down only. Send Cancel on focus loss before synthetic key releases.
    pub fn key(
        &mut self,
        world: &World,
        layout: &UiLayout,
        key: UiKey,
        pressed: bool,
        repeat: bool,
    ) -> Result<UiResponse, WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        let order: Vec<_> = targets
            .rects
            .iter()
            .map(|r| r.object)
            .filter(|o| targets.interactable(*o))
            .collect();
        self.focus = self.focus.filter(|o| order.contains(o));
        let mut response = UiResponse {
            consumed: self.focus.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        if !pressed {
            self.keys.remove(&key);
            return Ok(response);
        }
        if repeat || !self.keys.insert(key) {
            return Ok(response);
        }
        match key {
            UiKey::Tab if !order.is_empty() => {
                let reverse = self.keys.contains(&UiKey::Shift);
                let current = self.focus.and_then(|o| order.iter().position(|v| *v == o));
                let next = match (current, reverse) {
                    (Some(i), true) => (i + order.len() - 1) % order.len(),
                    (Some(i), false) => (i + 1) % order.len(),
                    (None, true) => order.len() - 1,
                    (None, false) => 0,
                };
                self.focus = Some(order[next]);
                response.consumed = true;
            }
            UiKey::Enter | UiKey::Space => {
                if let Some(object) = self.focus {
                    response.actions.push(self.activate(object));
                }
            }
            _ => {}
        }
        Ok(response)
    }

    fn cancel(&mut self) {
        self.capture = None;
        self.primary_held = false;
        self.position = None;
        self.focus = None;
        self.keys.clear();
    }
    fn activate(&mut self, object: ObjectHandle) -> UiAction {
        match self.controls[&object].kind {
            UiControlKind::Button => UiAction::Activated { object },
            UiControlKind::Choice { group, index } => {
                self.choices.insert(group, index);
                UiAction::ChoiceSelected { group, index }
            }
        }
    }
}
