use super::{
    TargetId, Targets, UiControlKind, UiKey, UiResponse, UiRouter, groups_by_object,
    semantics::{UiAccessAction, UiAccessRequest},
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
        self.retain_focus(|object| targets.interactable(object));
        let Some(target) = request.target.target() else {
            return Ok(UiResponse::default());
        };
        if !targets.interactable(target) || !self.supports_access_action(target, request.action) {
            return Ok(UiResponse::default());
        }
        if self.focus != Some(target) {
            self.cancel_preedits();
        }
        self.focus = Some(target);
        let action = match request.action {
            UiAccessAction::Focus => None,
            UiAccessAction::Press | UiAccessAction::Toggle | UiAccessAction::Select => {
                self.activate(target)
            }
            UiAccessAction::Increment => self.slider_key(UiKey::Right),
            UiAccessAction::Decrement => self.slider_key(UiKey::Left),
            UiAccessAction::SetValue(value) => self.set_slider(target.object, value),
        };
        self.reveal_target(world, layout, target, &targets)?;
        Ok(UiResponse {
            consumed: true,
            actions: action.into_iter().collect(),
        })
    }
    fn supports_access_action(&self, target: TargetId, action: UiAccessAction) -> bool {
        if target.link.is_some() {
            return matches!(action, UiAccessAction::Focus | UiAccessAction::Press);
        }
        let kind = self.controls[&target.object].kind;
        match action {
            UiAccessAction::Focus => true,
            UiAccessAction::Press => !matches!(
                kind,
                UiControlKind::Text | UiControlKind::TextField | UiControlKind::Slider(_)
            ),
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
