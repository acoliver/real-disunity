//! Unity UI: canvases, their scalers, rect transforms and text.

use super::{CANVAS, ComponentError};
use crate::{
    package::{PackageAsset, PackageFont, PackageScript},
    scene::{
        ObjectId, Reference, Scene, Value,
        read::{RGBA, XYZW, flag, integer, number, object_id, text, variant, vector},
        views::RECT_TRANSFORM,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    ScreenSpaceOverlay,
    ScreenSpaceCamera,
    WorldSpace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub render_mode: RenderMode,
    /// The camera a screen-space-camera canvas renders through.
    pub camera: Option<ObjectId>,
    pub plane_distance: f64,
    pub pixel_perfect: bool,
    pub sorting_layer_id: i64,
    pub sorting_order: i64,
    pub target_display: i64,
}

/// The layout rectangle of a UI object, relative to its parent's rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct RectTransform {
    pub anchor_min: [f64; 2],
    pub anchor_max: [f64; 2],
    pub anchored_position: [f64; 2],
    pub size_delta: [f64; 2],
    pub pivot: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMatch {
    MatchWidthOrHeight,
    Expand,
    Shrink,
}

/// How a `CanvasScaler` sizes its canvas. Physical-size scaling depends on screen DPI and is not
/// supported.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasScale {
    ConstantPixelSize {
        scale_factor: f64,
    },
    ScaleWithScreenSize {
        reference_resolution: [f64; 2],
        screen_match: ScreenMatch,
        match_width_or_height: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanvasScaler {
    pub scale: CanvasScale,
    pub reference_pixels_per_unit: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Bold,
    Italic,
    BoldAndItalic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
    UpperLeft,
    UpperCenter,
    UpperRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    LowerLeft,
    LowerCenter,
    LowerRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalOverflow {
    Wrap,
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalOverflow {
    Truncate,
    Overflow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontData {
    pub font: PackageFont,
    pub size: i64,
    pub style: FontStyle,
    pub best_fit: bool,
    pub min_size: i64,
    pub max_size: i64,
    pub alignment: TextAnchor,
    pub align_by_geometry: bool,
    pub rich_text: bool,
    pub horizontal_overflow: HorizontalOverflow,
    pub vertical_overflow: VerticalOverflow,
    pub line_spacing: f64,
}

/// A UI `Text` drawn with the default UI material.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    /// Linear RGBA color.
    pub color: [f64; 4],
    pub font: FontData,
    pub raycast_target: bool,
}

const ANCHORS: [TextAnchor; 9] = [
    TextAnchor::UpperLeft,
    TextAnchor::UpperCenter,
    TextAnchor::UpperRight,
    TextAnchor::MiddleLeft,
    TextAnchor::MiddleCenter,
    TextAnchor::MiddleRight,
    TextAnchor::LowerLeft,
    TextAnchor::LowerCenter,
    TextAnchor::LowerRight,
];

impl Scene {
    pub fn canvas(&self, id: ObjectId) -> Result<Canvas, ComponentError> {
        let fields = self.expect_class(id, CANVAS, "Canvas")?;
        let modes = [
            RenderMode::ScreenSpaceOverlay,
            RenderMode::ScreenSpaceCamera,
            RenderMode::WorldSpace,
        ];
        Ok(Canvas {
            render_mode: self.field(
                id,
                "m_RenderMode",
                variant(fields, "m_RenderMode", &modes, 0),
            )?,
            camera: fields.get("m_Camera").and_then(object_id),
            plane_distance: self.field(
                id,
                "m_PlaneDistance",
                number(fields, "m_PlaneDistance", 100.0),
            )?,
            pixel_perfect: self.field(
                id,
                "m_PixelPerfect",
                flag(fields, "m_PixelPerfect", false),
            )?,
            sorting_layer_id: self.field(
                id,
                "m_SortingLayerID",
                integer(fields, "m_SortingLayerID", 0),
            )?,
            sorting_order: self.field(
                id,
                "m_SortingOrder",
                integer(fields, "m_SortingOrder", 0),
            )?,
            target_display: self.field(
                id,
                "m_TargetDisplay",
                integer(fields, "m_TargetDisplay", 0),
            )?,
        })
    }

    pub fn rect_transform(&self, id: ObjectId) -> Result<RectTransform, ComponentError> {
        let fields = self.expect_class(id, RECT_TRANSFORM, "RectTransform")?;
        let read = |key, default| self.field(id, key, vector(fields, key, XYZW, default));
        Ok(RectTransform {
            anchor_min: read("m_AnchorMin", [0.5; 2])?,
            anchor_max: read("m_AnchorMax", [0.5; 2])?,
            anchored_position: read("m_AnchoredPosition", [0.0; 2])?,
            size_delta: read("m_SizeDelta", [100.0; 2])?,
            pivot: read("m_Pivot", [0.5; 2])?,
        })
    }

    pub fn canvas_scaler(&self, id: ObjectId) -> Result<CanvasScaler, ComponentError> {
        let fields = self.expect_script(id, PackageScript::CanvasScaler)?;
        let scale = match self.field(id, "m_UiScaleMode", integer(fields, "m_UiScaleMode", 0))? {
            0 => CanvasScale::ConstantPixelSize {
                scale_factor: self.field(
                    id,
                    "m_ScaleFactor",
                    number(fields, "m_ScaleFactor", 1.0),
                )?,
            },
            1 => self.screen_size_scale(id, fields)?,
            mode => {
                return Err(self.component_error(
                    id,
                    format!("UI scale mode {mode} (constant physical size) is not supported"),
                ));
            }
        };
        Ok(CanvasScaler {
            scale,
            reference_pixels_per_unit: self.field(
                id,
                "m_ReferencePixelsPerUnit",
                number(fields, "m_ReferencePixelsPerUnit", 100.0),
            )?,
        })
    }

    fn screen_size_scale(
        &self,
        id: ObjectId,
        fields: &Value,
    ) -> Result<CanvasScale, ComponentError> {
        let matches = [
            ScreenMatch::MatchWidthOrHeight,
            ScreenMatch::Expand,
            ScreenMatch::Shrink,
        ];
        Ok(CanvasScale::ScaleWithScreenSize {
            reference_resolution: self.field(
                id,
                "m_ReferenceResolution",
                vector(fields, "m_ReferenceResolution", XYZW, [800.0, 600.0]),
            )?,
            screen_match: self.field(
                id,
                "m_ScreenMatchMode",
                variant(fields, "m_ScreenMatchMode", &matches, 0),
            )?,
            match_width_or_height: self.field(
                id,
                "m_MatchWidthOrHeight",
                number(fields, "m_MatchWidthOrHeight", 0.0),
            )?,
        })
    }

    pub fn text(&self, id: ObjectId) -> Result<Text, ComponentError> {
        let fields = self.expect_script(id, PackageScript::Text)?;
        if !matches!(
            fields.get("m_Material").and_then(Value::as_reference),
            None | Some(Reference::Null)
        ) {
            return Err(self.component_error(id, "custom UI text materials are not supported"));
        }
        let data = fields
            .get("m_FontData")
            .ok_or_else(|| self.component_error(id, "has no m_FontData"))?;
        Ok(Text {
            text: text(fields, "m_Text").unwrap_or_default(),
            color: self.field(id, "m_Color", vector(fields, "m_Color", RGBA, [1.0; 4]))?,
            font: self.font_data(id, data)?,
            raycast_target: self.field(
                id,
                "m_RaycastTarget",
                flag(fields, "m_RaycastTarget", true),
            )?,
        })
    }

    fn font_data(&self, id: ObjectId, data: &Value) -> Result<FontData, ComponentError> {
        let font = match data.get("m_Font").and_then(Value::as_reference) {
            Some(Reference::Package(PackageAsset::Font(font))) => font,
            other => {
                return Err(self.component_error(id, format!("font {other:?} is not supported")));
            }
        };
        let styles = [
            FontStyle::Normal,
            FontStyle::Bold,
            FontStyle::Italic,
            FontStyle::BoldAndItalic,
        ];
        let wraps = [HorizontalOverflow::Wrap, HorizontalOverflow::Overflow];
        let truncates = [VerticalOverflow::Truncate, VerticalOverflow::Overflow];
        let int = |key, default| self.field(id, key, integer(data, key, default));
        let bit = |key, default| self.field(id, key, flag(data, key, default));
        Ok(FontData {
            font,
            size: int("m_FontSize", 14)?,
            style: self.field(id, "m_FontStyle", variant(data, "m_FontStyle", &styles, 0))?,
            best_fit: bit("m_BestFit", false)?,
            min_size: int("m_MinSize", 10)?,
            max_size: int("m_MaxSize", 40)?,
            alignment: self.field(id, "m_Alignment", variant(data, "m_Alignment", &ANCHORS, 0))?,
            align_by_geometry: bit("m_AlignByGeometry", false)?,
            rich_text: bit("m_RichText", true)?,
            horizontal_overflow: self.field(
                id,
                "m_HorizontalOverflow",
                variant(data, "m_HorizontalOverflow", &wraps, 0),
            )?,
            vertical_overflow: self.field(
                id,
                "m_VerticalOverflow",
                variant(data, "m_VerticalOverflow", &truncates, 0),
            )?,
            line_spacing: self.field(id, "m_LineSpacing", number(data, "m_LineSpacing", 1.0))?,
        })
    }
}
