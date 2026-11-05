use super::{Targets, UiControlKind, UiRouter, groups_by_object};
use crate::ui::contribution::UiRenderPlan;
use crate::{ObjectHandle, World, WorldError};

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
            if let Some(tint) = self.tint(graphic.graphic.object, hover, &targets) {
                for quad in &mut graphic.graphic.quads {
                    quad.color = std::array::from_fn(|i| quad.color[i] * tint[i]);
                }
            }
        }
        for text in &mut plan.texts {
            if let Some(tint) = self.tint(text.object, hover, &targets) {
                text.color = std::array::from_fn(|i| text.color[i] * tint[i]);
            }
        }
        Ok(())
    }
    fn tint(
        &self,
        object: ObjectHandle,
        hover: Option<ObjectHandle>,
        targets: &Targets<'_>,
    ) -> Option<[f32; 4]> {
        let style = self.styles.get(&object)?;
        let control = self.controls.get(&object)?;
        let selected = match control.kind {
            UiControlKind::Choice { group, index } => self.selected(group) == Some(index),
            UiControlKind::ListItem { list, index } => self.selected(list) == Some(index),
            UiControlKind::Toggle { on } => on,
            UiControlKind::Button | UiControlKind::Slider(_) => false,
        };
        Some(if !targets.interactable(object) {
            style.disabled
        } else if self.capture == Some(object) {
            style.pressed
        } else if self.focus == Some(object) {
            style.focused
        } else if hover == Some(object) {
            style.hovered
        } else if selected {
            style.selected
        } else {
            style.normal
        })
    }
}
