use super::{Targets, UiControlKind, UiKey, UiResponse, UiRouter, groups_by_object};
use crate::ui::UiLayout;
use crate::{ObjectHandle, World, WorldError};

impl UiRouter {
    /// Activation occurs only on first key-down. Navigation accepts repeat observations.
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
        self.modal_changed(targets.modal);
        let order: Vec<_> = targets
            .rects
            .iter()
            .map(|r| r.object)
            .filter(|o| targets.interactable(*o))
            .collect();
        self.retain_focus(|o| order.contains(&o));
        let mut response = UiResponse {
            consumed: self.focus.is_some() || targets.modal.is_some(),
            actions: Vec::new(),
        };
        if self.field_key(key, pressed, repeat, &mut response) {
            return Ok(response);
        }
        if !pressed {
            self.keys.remove(&key);
            return Ok(response);
        }
        let navigation = matches!(
            key,
            UiKey::Left | UiKey::Right | UiKey::Up | UiKey::Down | UiKey::Home | UiKey::End
        );
        let fresh = self.keys.insert(key);
        if (!fresh || repeat) && !(repeat && navigation) {
            return Ok(response);
        }
        match key {
            UiKey::Tab if !order.is_empty() => {
                self.tab(&order);
                response.consumed = true;
            }
            UiKey::Enter | UiKey::Space => {
                if let Some(object) = self.focus {
                    response.actions.extend(self.activate(object));
                }
            }
            _ if navigation => {
                response.actions.extend(self.slider_key(key));
                if let Some(object) = self.navigate_list(&order, key) {
                    self.focus = Some(object);
                    response.actions.extend(self.activate(object));
                }
            }
            _ => {}
        }
        if let Some(object) = self.focus {
            self.layout.reveal(world, layout, object)?;
        }
        Ok(response)
    }
    fn tab(&mut self, order: &[ObjectHandle]) {
        let reverse = self.keys.contains(&UiKey::Shift);
        let current = self.focus.and_then(|o| order.iter().position(|v| *v == o));
        let next = match (current, reverse) {
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (Some(i), false) => (i + 1) % order.len(),
            (None, true) => order.len() - 1,
            (None, false) => 0,
        };
        self.cancel_preedits();
        self.focus = Some(order[next]);
    }
    fn navigate_list(&self, order: &[ObjectHandle], key: UiKey) -> Option<ObjectHandle> {
        let focus = self.focus?;
        let UiControlKind::ListItem { list, .. } = self.controls[&focus].kind else {
            return None;
        };
        let mut items: Vec<_> = order
            .iter()
            .filter_map(|&object| match self.controls[&object].kind {
                UiControlKind::ListItem { list: owner, index } if list == owner => {
                    Some((index, object))
                }
                _ => None,
            })
            .collect();
        items.sort_by_key(|entry| entry.0);
        let current = items.iter().position(|(_, object)| *object == focus)?;
        let index = match key {
            UiKey::Up => current.saturating_sub(1),
            UiKey::Down => (current + 1).min(items.len() - 1),
            UiKey::Home => 0,
            UiKey::End => items.len() - 1,
            _ => return None,
        };
        Some(items[index].1)
    }
}
