//! CPU UI layout. Coordinates within a rectangle are X-right/Y-up; camera planes
//! orient that basis into the engine world. Screen roots own their effective TRS,
//! never rewriting the authored transform. Text supports printable ASCII, Latin-1 and LF/CR,
//! with word wrapping, structured colour spans and font kerning, without shaping or substitution.

pub mod annotations;
mod clipboard;
pub use clipboard::{
    Clipboard, ClipboardCommand, ClipboardError, MemoryClipboard, ShortcutPlatform,
};
mod atlas;
mod clip;
mod faces;
mod faces_render;
mod font_data;
mod fonts;
mod geometry;
mod graphics;
mod group;
mod interaction;
mod layout;
mod layout_groups;
mod positioning;
pub mod presentation;
mod scroll;
pub use interaction::{
    UiAccessAction, UiAccessRequest, UiAnnouncement, UiNodeId, UiRole, UiSemanticNode,
    UiSemanticTree, UiSemanticUpdate, UiSemanticValue, UiSemantics, UiStyle,
};
pub use interaction::{UiAction, UiControl, UiControlKind, UiKey, UiResponse, UiRouter, UiSlider};
pub use layout_groups::{UiAlign, UiAxis, UiGridFlow, UiLayoutGroup, UiLayoutRules, UiSize};
pub use scroll::UiScrollView;
mod renderer;
mod sdf;
mod text;
mod text_cache;
mod text_field;
mod text_geometry;
pub use text_cache::TextCacheStats;
pub use text_field::{TextFieldStyle, TextReject, UiTextField};
mod wrap;
use crate::{ObjectHandle, WorldError};
pub use atlas::GlyphAtlas;
pub use faces::{FaceBook, FaceId, FaceInfo, FaceLimits, GlyphMetrics, GlyphPair, MAX_POINT_SIZE};
pub use faces_render::{GlyphJob, GlyphRect, RenderMode};
pub use fonts::{FontBook, UiLimits};
pub use geometry::{
    GeometryQuad, GeometryQuads, MAX_UI_INDICES, MAX_UI_VERTICES, UiGeometry, UiVertex,
};
use glam::{UVec2, Vec2};
pub use graphics::{UiGraphic, UiGraphicQuad, append_graphics, extract_graphics};
pub use group::{CanvasGroup, CanvasGroupFlags};
pub use layout::{RectExtent, RectLayout, UiLayout};
pub use renderer::{CanvasRenderer, CanvasRendererFlags, MAX_CANVAS_MATERIALS, inherited_alpha};
pub use text::{
    GlyphLayout, HorizontalAlign, HorizontalOverflow, LineMetrics, TextLayout, TextRun, TextSpan,
    UiLinkId, VerticalAlign, VerticalOverflow,
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
    /// Any finite values are a rectangle. Anchors and pivot may lie outside 0 to 1, and a minimum
    /// anchor may pass its maximum: scripts set the two anchors one at a time, and the layout keeps
    /// the negative size such values give.
    pub(crate) fn validate(self) -> Result<(), UiError> {
        if [
            self.anchor_min,
            self.anchor_max,
            self.pivot,
            self.size_delta,
            self.anchored_position,
        ]
        .iter()
        .any(|v| !v.is_finite())
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
    /// Unity's `Canvas.overrideSorting`: a nested canvas sorts by its own layer and order
    /// instead of its parent's. The engine stores it; sorting does not read it yet.
    pub override_sorting: bool,
    /// Unity's `Canvas.targetDisplay`, stored and not used: the engine has one display.
    pub target_display: u32,
    /// The flags of Unity's `AdditionalCanvasShaderChannels` (`TexCoord1` = 1, `TexCoord2` = 2,
    /// `TexCoord3` = 4, `Normal` = 8, `Tangent` = 16), stored and not used.
    pub additional_shader_channels: u32,
    /// Unity's `Canvas.referencePixelsPerUnit`, stored and not used by layout.
    pub reference_pixels_per_unit: f32,
}
/// Every defined bit of [`Canvas::additional_shader_channels`].
const SHADER_CHANNELS: u32 = 0b1_1111;
impl Default for Canvas {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: CanvasMode::default(),
            scale: CanvasScale::default(),
            sorting_layer: 0,
            sorting_order: 0,
            pixel_perfect: false,
            override_sorting: false,
            target_display: 0,
            additional_shader_channels: 0,
            reference_pixels_per_unit: 100.,
        }
    }
}
impl Canvas {
    pub(crate) fn validate(self) -> Result<(), UiError> {
        self.scale.factor(UVec2::new(800, 600))?;
        if !self.reference_pixels_per_unit.is_finite()
            || self.reference_pixels_per_unit <= 0.
            || self.additional_shader_channels & !SHADER_CHANNELS != 0
        {
            return Err(UiError::InvalidCanvas);
        }
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
