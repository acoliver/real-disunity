use super::{TargetId, Targets, UiControlKind, UiRouter, groups_by_object};
use crate::ui::contribution::UiRenderPlan;
use crate::{World, WorldError};

/// Caller-authored multiplicative tints. Priority: disabled, pressed, focused, hovered, selected,
/// normal. Apply once to a freshly extracted plan, after routing input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiStyle {
    pub normal: [f32; 4],
    pub focused: [f32; 4],
    pub selected: [f32; 4],
    pub hovered: [f32; 4],
    pub pressed: [f32; 4],
    pub disabled: [f32; 4],
}
impl Default for UiStyle {
    fn default() -> Self {
        Self {
            normal: [1.; 4],
            focused: [1.; 4],
            selected: [1.; 4],
            hovered: [1.; 4],
            pressed: [1.; 4],
            disabled: [1.; 4],
        }
    }
}
impl UiRouter {
    pub fn style_plan(&self, world: &World, plan: &mut UiRenderPlan) -> Result<(), WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, &plan.layout, self, &groups)?;
        let hover = self.position.and_then(|p| targets.hit(p));
        for graphic in &mut plan.graphics {
            let id = TargetId {
                object: graphic.graphic.object,
                link: graphic.link,
            };
            if let Some(tint) = self.tint(id, hover, &targets) {
                for vertex in graphic.graphic.triangles.iter_mut().flatten() {
                    multiply(&mut vertex.color, tint);
                }
                for quad in &mut graphic.graphic.quads {
                    multiply(&mut quad.color, tint);
                }
            }
        }
        for text in &mut plan.texts {
            if let Some(tint) = self.tint(text.object.into(), hover, &targets) {
                multiply(&mut text.color, tint);
            }
            if self
                .controls
                .get(&text.object)
                .is_none_or(|c| c.kind != UiControlKind::Text)
            {
                continue;
            }
            for quad in &mut text.quads {
                if let Some(link) = quad.link {
                    let id = TargetId {
                        object: text.object,
                        link: Some(link),
                    };
                    if let Some(tint) = self.tint(id, hover, &targets) {
                        multiply(&mut quad.color, tint);
                    }
                }
            }
        }
        Ok(())
    }
    fn tint(
        &self,
        mut id: TargetId,
        hover: Option<TargetId>,
        targets: &Targets<'_>,
    ) -> Option<[f32; 4]> {
        let control = self.controls.get(&id.object)?;
        if control.kind != UiControlKind::Text {
            id.link = None;
        }
        if id.link.is_none() && control.kind == UiControlKind::Text {
            return None;
        }
        let style = id
            .link
            .and_then(|link| self.link_styles.get(&(id.object, link)))
            .or_else(|| self.styles.get(&id.object))?;
        let selected = match control.kind {
            UiControlKind::Choice { group, index } => self.selected(group) == Some(index),
            UiControlKind::ListItem { list, index } => self.selected(list) == Some(index),
            UiControlKind::Toggle { on } => on,
            UiControlKind::Text
            | UiControlKind::Button
            | UiControlKind::Slider(_)
            | UiControlKind::TextField => false,
        };
        Some(if !targets.interactable(id) {
            style.disabled
        } else if self.capture == Some(id) {
            style.pressed
        } else if self.focus == Some(id) {
            style.focused
        } else if hover == Some(id) {
            style.hovered
        } else if selected {
            style.selected
        } else {
            style.normal
        })
    }
}
fn multiply(color: &mut [f32; 4], tint: [f32; 4]) {
    *color = std::array::from_fn(|i| color[i] * tint[i]);
}
