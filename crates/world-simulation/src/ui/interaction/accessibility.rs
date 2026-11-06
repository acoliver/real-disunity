use super::{
    Targets, UiControlKind, UiKey, UiResponse, UiRouter, groups_by_object,
    semantics::{UiAccessAction, UiAccessRequest, UiNodeId},
};
use crate::{World, WorldError, ui::UiLayout};

impl UiRouter {
    /// Assistive input shares eligibility, focus/reveal and control mutation with pointer and
    /// keyboard input. Obsolete, foreign, modal-excluded and unsupported requests do nothing.
    pub fn accessibility_action(
        &mut self,
        world: &World,
        layout: &UiLayout,
        request: UiAccessRequest,
    ) -> Result<UiResponse, WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        self.modal_changed(targets.modal);
        self.focus = self.focus.filter(|object| targets.interactable(*object));
        let UiNodeId::Object(object) = request.target else {
            return Ok(UiResponse::default());
        };
        if !targets.interactable(object) || !self.supports_access_action(object, request.action) {
            return Ok(UiResponse::default());
        }
        self.focus = Some(object);
        let action = match request.action {
            UiAccessAction::Focus => None,
            UiAccessAction::Press | UiAccessAction::Toggle | UiAccessAction::Select => {
                self.activate(object)
            }
            UiAccessAction::Increment => self.slider_key(UiKey::Right),
            UiAccessAction::Decrement => self.slider_key(UiKey::Left),
            UiAccessAction::SetValue(value) => self.set_slider(object, value),
        };
        self.layout.reveal(world, layout, object)?;
        Ok(UiResponse {
            consumed: true,
            actions: action.into_iter().collect(),
        })
    }
    fn supports_access_action(&self, object: crate::ObjectHandle, action: UiAccessAction) -> bool {
        let kind = self.controls[&object].kind;
        match action {
            UiAccessAction::Focus => true,
            UiAccessAction::Press => !matches!(kind, UiControlKind::Slider(_)),
            UiAccessAction::Toggle => matches!(kind, UiControlKind::Toggle { .. }),
            UiAccessAction::Select => matches!(
                kind,
                UiControlKind::Choice { .. } | UiControlKind::ListItem { .. }
            ),
            UiAccessAction::Increment | UiAccessAction::Decrement => {
                matches!(kind, UiControlKind::Slider(_))
            }
            UiAccessAction::SetValue(value) => {
                value.is_finite() && matches!(kind, UiControlKind::Slider(_))
            }
        }
    }
}
