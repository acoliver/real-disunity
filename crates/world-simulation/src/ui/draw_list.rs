//! Immediate physical-pixel overlay snapshots built on the existing UI presentation.
//! Submission order is painter order. Images are caller-owned immutable generations;
//! glyph coverage is shared with the borrowed `FontBook` and owned by finished snapshots.
mod images;
mod shapes;
mod slices;
mod text;
mod types;
pub use types::*;

use super::{
    FontBook, UiError, UiTexture,
    presentation::{UiDraw, UiDrawKind, UiPresentation},
};
use glam::{UVec2, Vec2};
use std::{ops::Range, sync::Arc};

pub struct UiDrawList<'fonts> {
    fonts: &'fonts FontBook,
    viewport: UVec2,
    order: UiOrder,
    limits: UiDrawLimits,
    clips: Vec<UiRect>,
    modifiers: Vec<UiColorModifier>,
    presentation: UiPresentation,
    characters: usize,
    atlas_pixels: usize,
    atlases: Vec<Arc<[u8]>>,
}
impl<'fonts> UiDrawList<'fonts> {
    pub fn new(
        viewport: UVec2,
        fonts: &'fonts FontBook,
        order: UiOrder,
        mut limits: UiDrawLimits,
    ) -> Result<Self, UiError> {
        if viewport.min_element() == 0 || viewport.max_element() > 16_384 {
            return Err(UiError::InvalidViewport);
        }
        if limits.max_stack_depth == 0 {
            return Err(UiError::StackLimit);
        }
        limits.max_vertices = limits
            .max_vertices
            .min(super::annotations::MAX_ANNOTATION_VERTICES);
        limits.max_characters = limits.max_characters.min(fonts.limits().max_glyphs);
        limits.max_atlas_pixels = limits.max_atlas_pixels.min(fonts.limits().max_atlas_pixels);
        Ok(Self {
            fonts,
            viewport,
            order,
            limits,
            clips: vec![UiRect {
                origin: Vec2::ZERO,
                size: viewport.as_vec2(),
            }],
            modifiers: vec![UiColorModifier::default()],
            presentation: UiPresentation::default(),
            characters: 0,
            atlas_pixels: 0,
            atlases: Vec::new(),
        })
    }
    pub fn push_clip(&mut self, rect: UiRect) -> Result<(), UiError> {
        rect.validate()?;
        if self.clips.len() >= self.limits.max_stack_depth {
            return Err(UiError::StackLimit);
        }
        self.clips
            .push(self.clips.last().expect("root clip").intersect(rect));
        Ok(())
    }
    /// Push a clip bounded only by the viewport, independently of the active clip.
    /// Popping restores the previous clip, including any ancestor intersections.
    pub fn push_clip_replace(&mut self, rect: UiRect) -> Result<(), UiError> {
        rect.validate()?;
        if self.clips.len() >= self.limits.max_stack_depth {
            return Err(UiError::StackLimit);
        }
        self.clips.push(self.clips[0].intersect(rect));
        Ok(())
    }
    pub fn pop_clip(&mut self) -> Result<(), UiError> {
        if self.clips.len() == 1 {
            return Err(UiError::StackUnderflow);
        }
        self.clips.pop();
        Ok(())
    }
    pub fn set_color(&mut self, color: [f32; 4]) -> Result<(), UiError> {
        check_color(color)?;
        self.modifiers.last_mut().expect("root modifier").color = color;
        Ok(())
    }
    pub fn set_alpha(&mut self, alpha: f32) -> Result<(), UiError> {
        check_color([alpha; 4])?;
        self.modifiers.last_mut().expect("root modifier").alpha = alpha;
        Ok(())
    }
    pub fn push_modifier(&mut self, modifier: UiColorModifier) -> Result<(), UiError> {
        check_color(modifier.color)?;
        check_color([modifier.alpha; 4])?;
        if self.modifiers.len() >= self.limits.max_stack_depth {
            return Err(UiError::StackLimit);
        }
        let current = self.modifiers.last().expect("root modifier");
        self.modifiers.push(UiColorModifier {
            color: multiply(current.color, modifier.color),
            alpha: current.alpha * modifier.alpha,
        });
        Ok(())
    }
    pub fn pop_modifier(&mut self) -> Result<(), UiError> {
        if self.modifiers.len() == 1 {
            return Err(UiError::StackUnderflow);
        }
        self.modifiers.pop();
        Ok(())
    }
    pub fn draw_range(&self) -> Range<usize> {
        self.order.first_hierarchy..self.order.first_hierarchy + self.presentation.draws.len()
    }
    pub fn finish(self) -> Result<UiPresentation, UiError> {
        if self.clips.len() != 1 || self.modifiers.len() != 1 {
            return Err(UiError::UnbalancedStack);
        }
        Ok(self.presentation)
    }
    fn clip(&self) -> [u32; 4] {
        let clip = self.clips.last().expect("root clip");
        let extent = self.viewport.as_vec2();
        let low = Vec2::new(clip.origin.x, extent.y - clip.origin.y - clip.size.y)
            .ceil()
            .clamp(Vec2::ZERO, extent)
            .as_uvec2();
        let high = Vec2::new(clip.origin.x + clip.size.x, extent.y - clip.origin.y)
            .floor()
            .clamp(Vec2::ZERO, extent)
            .as_uvec2();
        let size = high.max(low) - low;
        [low.x, low.y, size.x, size.y]
    }
    fn hidden(&self, destination: UiRect) -> bool {
        let clip = self.clip();
        destination.empty() || clip[2] == 0 || clip[3] == 0
    }
    fn effective(&self, color: [f32; 4]) -> [f32; 4] {
        let modifier = self.modifiers.last().expect("root modifier");
        let mut color = multiply(color, modifier.color);
        color[3] *= modifier.alpha;
        color
    }
    fn check_count(&self, count: usize) -> Result<(), UiError> {
        let total = self
            .presentation
            .draws
            .len()
            .checked_add(count)
            .ok_or(UiError::DrawLimit)?;
        if total > self.limits.max_vertices / 6 {
            return Err(UiError::DrawLimit);
        }
        self.order
            .first_hierarchy
            .checked_add(total)
            .ok_or(UiError::HierarchyLimit)?;
        Ok(())
    }
    fn commit(&mut self, mut draws: Vec<UiDraw>) -> Result<(), UiError> {
        self.check_count(draws.len())?;
        let start = self.draw_range().end;
        for (i, draw) in draws.iter_mut().enumerate() {
            draw.hierarchy_order = start + i;
        }
        self.presentation.draws.extend(draws);
        Ok(())
    }
    fn quad(
        &self,
        points: [Vec2; 4],
        uv_rect: [f32; 4],
        color: [f32; 4],
        texture: Option<UiTexture>,
        kind: UiDrawKind,
    ) -> Result<UiDraw, UiError> {
        let height = self.viewport.as_vec2().y;
        if points.iter().any(|p| !p.is_finite()) {
            return Err(UiError::InvalidRect);
        }
        let corners = points.map(|p| [p.x, height - p.y, 0.]);
        if corners.iter().flatten().any(|v| !v.is_finite()) {
            return Err(UiError::InvalidRect);
        }
        Ok(UiDraw {
            kind,
            depth_test: false,
            triangle_uv: None,
            corners,
            uv_rect,
            color: self.effective(color),
            corner_colors: None,
            distance_field: None,
            texture,
            material: None,
            sorting_layer: self.order.layer,
            sorting_order: self.order.order,
            hierarchy_order: 0,
            clip_rect: self.clip(),
        })
    }
}
fn check_color(color: [f32; 4]) -> Result<(), UiError> {
    if color.into_iter().any(|v| !(0. ..=1.).contains(&v)) {
        return Err(UiError::InvalidColor);
    }
    Ok(())
}
fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|i| a[i] * b[i])
}
fn check_transform(angle: f32, pivot: Vec2) -> Result<(), UiError> {
    if !angle.is_finite()
        || !pivot.is_finite()
        || pivot.min_element() < 0.
        || pivot.max_element() > 1.
    {
        return Err(UiError::InvalidRect);
    }
    Ok(())
}
fn rotate(points: [Vec2; 4], destination: UiRect, angle: f32, pivot: Vec2) -> [Vec2; 4] {
    let pivot = destination.origin + destination.size * pivot;
    let (sin, cos) = angle.sin_cos();
    points.map(|p| {
        let v = p - pivot;
        pivot + Vec2::new(cos * v.x - sin * v.y, sin * v.x + cos * v.y)
    })
}
