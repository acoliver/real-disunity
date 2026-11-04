use super::{UiControlKind, UiKey, UiRouter};
use crate::ui::{UiLayout, text_field::pointer::PointerEdit};
use crate::{ObjectHandle, PointerEvent, World, WorldError};
use std::time::Duration;

#[derive(Default)]
pub(super) struct TextClicks {
    time: Option<Duration>,
    last: Option<(ObjectHandle, Duration, [f64; 2])>,
}
impl UiRouter {
    /// Monotonic host timestamps enable double-clicks. Untimed `pointer` still supports
    /// placement and dragging, but never infers a double-click from event frequency.
    pub fn pointer_at(
        &mut self,
        world: &World,
        layout: &UiLayout,
        event: PointerEvent,
        time: Duration,
    ) -> Result<super::UiResponse, WorldError> {
        self.text_clicks.time = Some(time);
        let response = self.pointer(world, layout, event);
        self.text_clicks.time = None;
        response
    }
    pub(super) fn pointer_field(&mut self, layout: &UiLayout, pressed: bool) {
        let Some(object) = self
            .capture
            .filter(|o| self.controls[o].kind == UiControlKind::TextField)
        else {
            if pressed {
                self.text_clicks.last = None;
            }
            return;
        };
        let Some(position) = self.position else {
            return;
        };
        let Some(rect) = layout.rect(object) else {
            return;
        };
        if rect.size.x <= 0. {
            return;
        }
        let point = glam::DVec2::new(position[0], f64::from(layout.viewport.y) - position[1]);
        let Some(local) = super::targets::coordinates(rect, point) else {
            return;
        };
        let edit = if !pressed {
            PointerEdit::Drag
        } else if self.keys.contains(&UiKey::Shift) {
            self.text_clicks.last = None;
            PointerEdit::Extend
        } else if self.double_click(object, position) {
            PointerEdit::Word
        } else {
            PointerEdit::Place
        };
        if let Some(field) = self.text_fields.get_mut(&object) {
            let local = local
                .clamp(glam::DVec2::splat(-1.), glam::DVec2::splat(2.))
                .as_vec2();
            field.pointer(local.x * rect.size.x, rect.size.x, edit);
        }
    }
    fn double_click(&mut self, object: ObjectHandle, position: [f64; 2]) -> bool {
        let Some(time) = self.text_clicks.time else {
            self.text_clicks.last = None;
            return false;
        };
        let double = self.text_clicks.last.is_some_and(|(o, t, p)| {
            o == object
                && time
                    .checked_sub(t)
                    .is_some_and(|dt| dt <= Duration::from_millis(500))
                && (position[0] - p[0]).hypot(position[1] - p[1]) <= 4.
        });
        self.text_clicks.last = if double {
            None
        } else {
            Some((object, time, position))
        };
        double
    }
}
