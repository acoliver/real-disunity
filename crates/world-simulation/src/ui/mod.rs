//! CPU UI layout. Coordinates within a rectangle are X-right/Y-up; camera planes
//! orient that basis into the engine world. Screen roots own their effective TRS,
//! never rewriting the authored transform. Text supports printable ASCII and LF/CR
//! only, with character wrapping and font kerning, without shaping or substitution.

mod atlas;
mod font_data;
mod fonts;
mod layout;
mod text;
mod wrap;
use crate::{ObjectHandle, WorldError};
pub use atlas::GlyphAtlas;
pub use fonts::{FontBook, UiLimits};
use glam::{UVec2, Vec2};
pub use layout::{RectLayout, UiLayout};
pub use text::{
    GlyphLayout, HorizontalAlign, HorizontalOverflow, LineMetrics, TextLayout, TextRun,
    VerticalAlign, VerticalOverflow,
};

pub use crate::{FontId, UiError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectTransform {
    pub anchor_min: Vec2,
    pub anchor_max: Vec2,
    pub pivot: Vec2,
    pub size_delta: Vec2,
    pub anchored_position: Vec2,
}
impl Default for RectTransform {
    fn default() -> Self {
        Self {
            anchor_min: Vec2::splat(0.5),
            anchor_max: Vec2::splat(0.5),
            pivot: Vec2::splat(0.5),
            size_delta: Vec2::splat(100.),
            anchored_position: Vec2::ZERO,
        }
    }
}
impl RectTransform {
    pub(crate) fn validate(self) -> Result<(), UiError> {
        if [self.anchor_min, self.anchor_max, self.pivot]
            .iter()
            .any(|v| !v.is_finite() || v.min_element() < 0. || v.max_element() > 1.)
            || self.anchor_min.cmpgt(self.anchor_max).any()
            || !self.size_delta.is_finite()
            || !self.anchored_position.is_finite()
        {
            return Err(UiError::InvalidRect);
        }
        Ok(())
    }
    pub fn with_offsets(mut self, min: Vec2, max: Vec2) -> Result<Self, UiError> {
        self.size_delta = max - min;
        self.anchored_position = min + self.pivot * self.size_delta;
        self.validate()?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CanvasMode {
    #[default]
    ScreenSpaceOverlay,
    ScreenSpaceCamera {
        camera: ObjectHandle,
        orthographic_size: f32,
        plane_distance: f32,
    },
    WorldSpace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasScale {
    Constant(f32),
    MatchViewport { reference: Vec2, match_height: f32 },
}
impl Default for CanvasScale {
    fn default() -> Self {
        Self::Constant(1.)
    }
}
impl CanvasScale {
    pub fn factor(self, viewport: UVec2) -> Result<f32, UiError> {
        let factor = match self {
            Self::Constant(factor) => factor,
            Self::MatchViewport {
                reference,
                match_height,
            } => {
                if !reference.is_finite()
                    || reference.min_element() <= 0.
                    || !(0. ..=1.).contains(&match_height)
                {
                    return Err(UiError::InvalidCanvas);
                }
                let ratio = viewport.as_vec2() / reference;
                ratio.x.powf(1. - match_height) * ratio.y.powf(match_height)
            }
        };
        if !factor.is_finite() || factor <= 0. {
            return Err(UiError::InvalidCanvas);
        }
        Ok(factor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub enabled: bool,
    pub mode: CanvasMode,
    pub scale: CanvasScale,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    /// Unity's `Canvas.pixelPerfect`: on a screen-space canvas, each text is moved so its first
    /// glyph starts on a whole screen pixel. World-space canvases ignore it, as Unity's do.
    pub pixel_perfect: bool,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: CanvasMode::default(),
            scale: CanvasScale::default(),
            sorting_layer: 0,
            sorting_order: 0,
            pixel_perfect: false,
        }
    }
}
impl Canvas {
    pub(crate) fn validate(self) -> Result<(), UiError> {
        self.scale.factor(UVec2::new(800, 600))?;
        if let CanvasMode::ScreenSpaceCamera {
            orthographic_size,
            plane_distance,
            ..
        } = self.mode
            && (!orthographic_size.is_finite()
                || orthographic_size <= 0.
                || !plane_distance.is_finite()
                || plane_distance <= 0.)
        {
            return Err(UiError::InvalidCanvas);
        }
        Ok(())
    }
}
impl From<UiError> for WorldError {
    fn from(value: UiError) -> Self {
        Self::Ui(value)
    }
}

pub mod contribution;
