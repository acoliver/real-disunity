use super::{Targets, UiResponse, UiRouter, groups_by_object};
use crate::ui::UiLayout;
use crate::{ObjectHandle, PointerButton, PointerEvent, World, WorldError};

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
        self.retain_focus(|o| targets.interactable(o));
        self.capture = self.capture.filter(|o| targets.interactable(*o));
        let hit = self.position.and_then(|position| targets.hit(position));
        let mut response = UiResponse {
            consumed: self.capture.is_some() || hit.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        match event {
            PointerEvent::Cancel => self.cancel(),
            PointerEvent::Moved(_) => {
                self.drag_slider(layout, &mut response);
                self.pointer_field(layout, false);
            }
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
                self.control_press((&targets, hit), layout, pressed, &mut response);
            }
            PointerEvent::Button { .. } => {}
        }
        Ok(response)
    }
    fn control_press(
        &mut self,
        target: (&Targets<'_>, Option<ObjectHandle>),
        layout: &UiLayout,
        pressed: bool,
        response: &mut UiResponse,
    ) {
        let fresh = pressed && !self.primary_held;
        self.primary_button(target.0, target.1, pressed, response);
        if fresh {
            self.pointer_field(layout, true);
        }
        if pressed {
            self.drag_slider(layout, response);
        }
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
                if self.focus != self.capture {
                    self.cancel_preedits();
                }
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
