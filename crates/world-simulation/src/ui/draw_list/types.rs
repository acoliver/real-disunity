use super::super::UiError;
use glam::{UVec2, Vec2};

/// Physical pixels, top-left origin, positive Y down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub origin: Vec2,
    pub size: Vec2,
}
impl UiRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Vec2::new(x, y),
            size: Vec2::new(width, height),
        }
    }
    pub(super) fn validate(self) -> Result<(), UiError> {
        if !self.origin.is_finite()
            || !self.size.is_finite()
            || self.size.min_element() < 0.
            || !(self.origin + self.size).is_finite()
        {
            return Err(UiError::InvalidRect);
        }
        Ok(())
    }
    pub(super) fn empty(self) -> bool {
        self.size.min_element() == 0.
    }
    pub(super) fn intersect(self, other: Self) -> Self {
        let origin = self.origin.max(other.origin);
        let end = (self.origin + self.size).min(other.origin + other.size);
        Self {
            origin,
            size: (end - origin).max(Vec2::ZERO),
        }
    }
    pub(super) fn points(self) -> [Vec2; 4] {
        let end = self.origin + self.size;
        [
            Vec2::new(self.origin.x, end.y),
            end,
            Vec2::new(end.x, self.origin.y),
            self.origin,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Immutable caller-owned image generation, resolved by the overlay's image map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiImageId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiImage {
    pub id: UiImageId,
    pub size: UVec2,
}
impl UiImage {
    pub fn new(id: UiImageId, size: UVec2) -> Result<Self, UiError> {
        if size.min_element() == 0 {
            return Err(UiError::InvalidImage);
        }
        Ok(Self { id, size })
    }
    pub(super) fn source(self, source: Option<UiRect>) -> Result<UiRect, UiError> {
        let extent = self.size.as_vec2();
        let source = source.unwrap_or(UiRect {
            origin: Vec2::ZERO,
            size: extent,
        });
        source.validate().map_err(|_| UiError::InvalidImage)?;
        if self.size.min_element() == 0
            || source.empty()
            || source.origin.min_element() < 0.
            || (source.origin + source.size).cmpgt(extent).any()
        {
            return Err(UiError::InvalidImage);
        }
        Ok(source)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiFlip {
    #[default]
    None,
    X,
    Y,
    XY,
}
impl UiFlip {
    pub(super) fn x(self) -> bool {
        matches!(self, Self::X | Self::XY)
    }
    pub(super) fn y(self) -> bool {
        matches!(self, Self::Y | Self::XY)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiImageOptions {
    pub source: Option<UiRect>,
    /// Linear RGBA in destination BL/BR/TR/TL order. Flips only affect sampling.
    pub colors: [[f32; 4]; 4],
    pub flip: UiFlip,
    /// Clockwise radians in screen space.
    pub rotation_radians: f32,
    pub pivot: Vec2,
}
impl Default for UiImageOptions {
    fn default() -> Self {
        Self {
            source: None,
            colors: [[1.; 4]; 4],
            flip: UiFlip::None,
            rotation_radians: 0.,
            pivot: Vec2::splat(0.5),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiBorder {
    /// Inward X/Y thickness, clamped to half the destination dimensions.
    pub width: Vec2,
    pub color: [f32; 4],
    pub fill: Option<[f32; 4]>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiSliceMode {
    #[default]
    Stretch,
    Tile,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiSliceCenter {
    #[default]
    Draw,
    Omit,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiNineSlice {
    pub source: Option<UiRect>,
    pub insets: UiInsets,
    pub scale: Vec2,
    pub mode: UiSliceMode,
    pub center: UiSliceCenter,
    pub color: [f32; 4],
    pub flip: UiFlip,
}
impl Default for UiNineSlice {
    fn default() -> Self {
        Self {
            source: None,
            insets: UiInsets::default(),
            scale: Vec2::ONE,
            mode: UiSliceMode::Stretch,
            center: UiSliceCenter::Draw,
            color: [1.; 4],
            flip: UiFlip::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum UiTextEffect {
    #[default]
    None,
    /// Offset in final screen axes, after stretching and rotation.
    Shadow { color: [f32; 4], offset: Vec2 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTextPlacement {
    pub layout_size: Vec2,
    pub destination: UiRect,
    pub rotation_radians: f32,
    pub pivot: Vec2,
}
impl UiTextPlacement {
    pub fn new(destination: UiRect) -> Self {
        Self {
            layout_size: destination.size,
            destination,
            rotation_radians: 0.,
            pivot: Vec2::splat(0.5),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiColorModifier {
    pub color: [f32; 4],
    pub alpha: f32,
}
impl Default for UiColorModifier {
    fn default() -> Self {
        Self {
            color: [1.; 4],
            alpha: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiOrder {
    pub layer: i32,
    pub order: i32,
    pub first_hierarchy: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiDrawLimits {
    /// Includes the root clip/modifier entry.
    pub max_stack_depth: usize,
    pub max_vertices: usize,
    pub max_atlas_pixels: usize,
    pub max_characters: usize,
}
impl Default for UiDrawLimits {
    fn default() -> Self {
        Self {
            max_stack_depth: 64,
            max_vertices: 65_536,
            max_atlas_pixels: 4 * 1024 * 1024,
            max_characters: 4096,
        }
    }
}
