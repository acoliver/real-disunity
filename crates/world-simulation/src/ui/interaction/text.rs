use super::{Targets, UiAction, UiControlKind, UiKey, UiResponse, UiRouter, groups_by_object};
use crate::ui::{FontBook, UiLayout};
use crate::{ObjectHandle, TextInput, World, WorldError};

impl UiRouter {
    pub(in crate::ui) fn text_target(
        &self,
        world: &World,
        layout: &UiLayout,
    ) -> Result<Option<ObjectHandle>, WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        Ok(self.focus.filter(|o| {
            targets.interactable(*o)
                && self
                    .controls
                    .get(o)
                    .is_some_and(|c| c.kind == UiControlKind::TextField)
                && self.text_fields.contains_key(o)
        }))
    }
    /// Route ordered committed/preedit input once, independently of held-key edges.
    pub fn text(
        &mut self,
        world: &World,
        layout: &UiLayout,
        fonts: &FontBook,
        event: &TextInput,
    ) -> Result<UiResponse, WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        self.modal_changed(targets.modal);
        self.retain_focus(|o| targets.interactable(o));
        let mut response = UiResponse {
            consumed: targets.modal.is_some(),
            actions: Vec::new(),
        };
        if let Some(control) = self.text_target(world, layout)? {
            response.consumed = true;
            match self
                .text_fields
                .get_mut(&control)
                .expect("registered field")
                .input(fonts, event)
            {
                Ok(true) => response.actions.push(UiAction::TextChanged { control }),
                Ok(false) => {}
                Err(reason) => response
                    .actions
                    .push(UiAction::TextRejected { control, reason }),
            }
        }
        Ok(response)
    }
    /// Physical top-left cursor rectangle for the host's IME area. None disables IME.
    pub fn text_input_area(
        &self,
        world: &World,
        layout: &UiLayout,
        fonts: &FontBook,
    ) -> Result<Option<[f64; 4]>, WorldError> {
        let Some(control) = self.text_target(world, layout)? else {
            return Ok(None);
        };
        let rect = layout.rect(control).expect("eligible rectangle");
        fonts.validate_single_line(self.text_fields[&control].text_run())?;
        Ok(Some(
            self.text_fields[&control].caret_area(rect, layout.viewport.as_vec2().y)?,
        ))
    }
    pub(super) fn field_key(
        &mut self,
        key: UiKey,
        pressed: bool,
        repeat: bool,
        response: &mut UiResponse,
    ) -> bool {
        if key == UiKey::Tab {
            return false;
        }
        let Some(control) = self
            .focus
            .filter(|o| self.controls[o].kind == UiControlKind::TextField)
        else {
            return false;
        };
        let Some(field) = self.text_fields.get_mut(&control) else {
            return false;
        };
        if !pressed {
            self.keys.remove(&key);
            return true;
        }
        let fresh = self.keys.insert(key);
        if !fresh && !repeat {
            return true;
        }
        if !field.preedit().is_empty() && key != UiKey::Escape {
            return true;
        }
        match key {
            UiKey::Enter if fresh && !repeat => {
                response.actions.push(UiAction::TextSubmitted { control });
            }
            UiKey::Escape if fresh && !repeat => {
                field.cancel_preedit();
                response.actions.push(UiAction::TextCancelled { control });
            }
            UiKey::Control
            | UiKey::Alt
            | UiKey::Shift
            | UiKey::Enter
            | UiKey::Escape
            | UiKey::Space => {}
            _ => {
                let shift = self.keys.contains(&UiKey::Shift);
                let word = self.keys.contains(&UiKey::Control) || self.keys.contains(&UiKey::Alt);
                if field.edit(key, shift, word) {
                    response.actions.push(UiAction::TextChanged { control });
                }
            }
        }
        true
    }
    pub(super) fn cancel_preedits(&mut self) {
        for field in self.text_fields.values_mut() {
            field.cancel_preedit();
        }
    }
}
