use super::{TargetId, Targets, UiResponse, UiRouter, groups_by_object};
use crate::ui::UiLayout;
use crate::{PointerButton, PointerEvent, World, WorldError};

impl UiRouter {
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
        self.capture = self.capture.filter(|o| targets.interactable(*o));
        let hit = self.position.and_then(|position| targets.hit(position));
        let mut response = UiResponse {
            consumed: self.capture.is_some() || hit.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        match event {
            PointerEvent::Cancel => self.cancel(),
            PointerEvent::Moved(_) => self.drag_slider(layout, &mut response),
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
                if pressed {
                    self.drag_slider(layout, &mut response);
                }
            }
            PointerEvent::Button { .. } => {}
        }
        Ok(response)
    }
    fn primary_button(
        &mut self,
        targets: &Targets<'_>,
        hit: Option<TargetId>,
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
                response.actions.extend(self.activate(object));
            }
        }
    }
}
