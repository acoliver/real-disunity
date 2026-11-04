use super::{UiError, UiLayout, UiLayoutRules};
use crate::{ObjectHandle, PointerWheel, World, WorldError};
use glam::{DVec2, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScrollView {
    /// A direct child of the viewport, positioned from its top left.
    pub content: ObjectHandle,
    /// Logical pixels per wheel line. Pixel wheel deltas retain their physical units.
    pub line_step: u16,
    offset: Vec2,
}
impl UiScrollView {
    pub fn new(content: ObjectHandle) -> Self {
        Self {
            content,
            line_step: 20,
            offset: Vec2::ZERO,
        }
    }
    pub fn offset(self) -> Vec2 {
        self.offset
    }
}
impl UiLayoutRules {
    /// Set a bounded top-left content offset using the current layout's extents.
    pub fn scroll_to(
        &mut self,
        layout: &UiLayout,
        viewport: ObjectHandle,
        offset: Vec2,
    ) -> Result<(), WorldError> {
        if !offset.is_finite() {
            return Err(UiError::InvalidRect.into());
        }
        let scroll = self
            .scrolls
            .get_mut(&viewport)
            .ok_or(UiError::MissingLayout)?;
        let view = layout.rect(viewport).ok_or(UiError::MissingLayout)?;
        let content = layout.rect(scroll.content).ok_or(UiError::MissingLayout)?;
        scroll.offset = offset
            .round()
            .clamp(Vec2::ZERO, (content.size - view.size).max(Vec2::ZERO));
        Ok(())
    }
    pub(super) fn clip(&self, world: &World, layout: &mut UiLayout) -> Result<(), WorldError> {
        let mut clips = Vec::new();
        for (&object, scroll) in &self.scrolls {
            let Some(view) = layout.rect(object) else {
                continue;
            };
            let corners = view.screen_corners.ok_or(UiError::InvalidRect)?;
            if view.mode != super::CanvasMode::ScreenSpaceOverlay
                || corners[1].x <= corners[0].x
                || corners[3].y <= corners[0].y
                || (corners[1].y - corners[0].y).abs() > 0.001
                || (corners[3].x - corners[0].x).abs() > 0.001
            {
                return Err(UiError::InvalidRect.into());
            }
            let min = corners.iter().copied().fold(Vec2::INFINITY, Vec2::min);
            let max = corners.iter().copied().fold(Vec2::NEG_INFINITY, Vec2::max);
            clips.push((scroll.content, [min, max]));
        }
        for rect in &mut layout.rects {
            let path = world.ancestry(rect.object)?;
            for &(_, [min, max]) in clips.iter().filter(|(content, _)| path.contains(content)) {
                let [low, high] = rect
                    .clip_bounds
                    .unwrap_or([Vec2::ZERO, layout.viewport.as_vec2()]);
                let low = low.max(min).min(high);
                rect.clip_bounds = Some([low, high.min(max).max(low)]);
            }
        }
        Ok(())
    }
    pub(super) fn wheel(
        &mut self,
        world: &World,
        layout: &UiLayout,
        position: [f64; 2],
        wheel: PointerWheel,
        modal: Option<ObjectHandle>,
    ) -> Result<bool, WorldError> {
        let point = DVec2::new(position[0], f64::from(layout.viewport.y) - position[1]).as_vec2();
        let mut views = Vec::new();
        for &object in self.scrolls.keys() {
            let Some(rect) = layout.rect(object) else {
                continue;
            };
            if let Some(modal) = modal
                && !world.ancestry(object)?.contains(&modal)
            {
                continue;
            }
            if super::interaction::contains(rect, point) {
                views.push(rect);
            }
        }
        views.sort_by_key(|r| r.draw_order());
        let Some(view) = views.last() else {
            return Ok(false);
        };
        let scroll = self.scrolls[&view.object];
        let delta = match wheel {
            PointerWheel::Lines(v) => DVec2::from(v).as_vec2() * f32::from(scroll.line_step),
            PointerWheel::Pixels(v) => {
                let corners = view.screen_corners.expect("screen scroll viewport");
                DVec2::from(v).as_vec2() * view.size
                    / Vec2::new(
                        (corners[1] - corners[0]).length(),
                        (corners[3] - corners[0]).length(),
                    )
            }
        };
        let max = (layout
            .rect(scroll.content)
            .ok_or(UiError::MissingLayout)?
            .size
            - view.size)
            .max(Vec2::ZERO);
        self.scroll_to(
            layout,
            view.object,
            (scroll.offset - delta).clamp(Vec2::ZERO, max),
        )?;
        Ok(true)
    }
    pub(super) fn reveal(
        &mut self,
        world: &World,
        layout: &UiLayout,
        object: ObjectHandle,
    ) -> Result<(), WorldError> {
        let path = world.ancestry(object)?;
        let mut current = layout.clone();
        for &viewport in path.iter().rev() {
            let Some(scroll) = self.scrolls.get(&viewport).copied() else {
                continue;
            };
            if !path.contains(&scroll.content) {
                continue;
            }
            let view = current.rect(viewport).ok_or(UiError::MissingLayout)?;
            let target = current.rect(object).ok_or(UiError::MissingLayout)?;
            let inverse = view.local_to_world.inverse();
            let points = target
                .corners
                .map(|p| inverse.transform_point3(p).truncate() + view.pivot * view.size);
            let low = points.into_iter().fold(Vec2::INFINITY, Vec2::min);
            let high = points.into_iter().fold(Vec2::NEG_INFINITY, Vec2::max);
            let shift = low.min(Vec2::ZERO) + (high - view.size).max(Vec2::ZERO);
            let max = (current
                .rect(scroll.content)
                .ok_or(UiError::MissingLayout)?
                .size
                - view.size)
                .max(Vec2::ZERO);
            let offset = (scroll.offset + shift * Vec2::new(1., -1.)).clamp(Vec2::ZERO, max);
            self.scroll_to(&current, viewport, offset)?;
            current = world.ui_layout_with(layout.viewport, self)?;
        }
        Ok(())
    }
}
