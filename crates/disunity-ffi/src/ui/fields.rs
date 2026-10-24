use glam::Vec2;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RectTransform {
    pub anchor_min: [f32; 2],
    pub anchor_max: [f32; 2],
    pub pivot: [f32; 2],
    pub size_delta: [f32; 2],
    pub anchored_position: [f32; 2],
}
impl Default for RectTransform {
    fn default() -> Self {
        Self {
            anchor_min: [0.5; 2],
            anchor_max: [0.5; 2],
            pivot: [0.5; 2],
            size_delta: [100.0; 2],
            anchored_position: [0.0; 2],
        }
    }
}
impl RectTransform {
    pub fn parse(mut value: Value) -> Result<Self, String> {
        let map = value
            .as_object_mut()
            .ok_or("rect fields must be an object")?;
        let min = map
            .remove("offset_min")
            .map(serde_json::from_value::<[f32; 2]>)
            .transpose()
            .map_err(|e| e.to_string())?;
        let max = map
            .remove("offset_max")
            .map(serde_json::from_value::<[f32; 2]>)
            .transpose()
            .map_err(|e| e.to_string())?;
        let mut r: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let (old_min, old_max) = r.offsets();
        if min.is_some() || max.is_some() {
            let min = Vec2::from(min.unwrap_or(old_min));
            let max = Vec2::from(max.unwrap_or(old_max));
            r.size_delta = (max - min).to_array();
            r.anchored_position = (min + Vec2::from(r.pivot) * (max - min)).to_array();
        }
        r.validate()?;
        Ok(r)
    }
    pub fn offsets(&self) -> ([f32; 2], [f32; 2]) {
        let size = Vec2::from(self.size_delta);
        let position = Vec2::from(self.anchored_position);
        let pivot = Vec2::from(self.pivot);
        (
            (position - pivot * size).to_array(),
            (position + (Vec2::ONE - pivot) * size).to_array(),
        )
    }
    fn validate(&self) -> Result<(), String> {
        if self
            .anchor_min
            .iter()
            .chain(self.anchor_max.iter())
            .chain(self.pivot.iter())
            .any(|v| !(0.0..=1.0).contains(v))
            || self
                .anchor_min
                .iter()
                .zip(self.anchor_max)
                .any(|(a, b)| *a > b)
            || self
                .size_delta
                .iter()
                .chain(self.anchored_position.iter())
                .any(|v| !v.is_finite())
        {
            return Err("invalid RectTransform anchors, pivot or offsets".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reference {
    #[serde(rename = "$ref")]
    pub handle: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RenderMode {
    #[default]
    ScreenSpaceOverlay,
    ScreenSpaceCamera,
    WorldSpace,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Canvas {
    pub render_mode: RenderMode,
    pub camera: Option<Reference>,
    pub plane_distance: f32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            render_mode: RenderMode::default(),
            camera: None,
            plane_distance: 100.0,
            sorting_layer: 0,
            sorting_order: 0,
        }
    }
}
impl Canvas {
    pub fn validate(&self) -> Result<(), String> {
        if !self.plane_distance.is_finite() || self.plane_distance <= 0.0 {
            return Err("invalid canvas plane distance".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScaleMode {
    #[default]
    ConstantPixelSize,
    ScaleWithScreenSize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CanvasScaler {
    pub scale_mode: ScaleMode,
    pub scale_factor: f32,
    pub reference_resolution: [f32; 2],
    pub match_width_or_height: f32,
}
impl Default for CanvasScaler {
    fn default() -> Self {
        Self {
            scale_mode: ScaleMode::default(),
            scale_factor: 1.0,
            reference_resolution: [800.0, 600.0],
            match_width_or_height: 0.0,
        }
    }
}
impl CanvasScaler {
    pub fn factor(&self, viewport: [u32; 2]) -> f32 {
        if self.scale_mode == ScaleMode::ConstantPixelSize {
            return self.scale_factor;
        }
        let ratio = Vec2::new(viewport[0] as f32, viewport[1] as f32)
            / Vec2::from(self.reference_resolution);
        ratio.x.powf(1.0 - self.match_width_or_height) * ratio.y.powf(self.match_width_or_height)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.scale_factor.is_finite()
            || self.scale_factor <= 0.0
            || self
                .reference_resolution
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0)
            || !(0.0..=1.0).contains(&self.match_width_or_height)
        {
            return Err("invalid CanvasScaler values".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Text {
    pub text: String,
    pub font: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub alignment: u8,
    pub line_spacing: f32,
    pub wrap: bool,
    pub overflow: bool,
}
impl Default for Text {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: "default".into(),
            font_size: 14.0,
            color: [1.0; 4],
            alignment: 0,
            line_spacing: 1.0,
            wrap: false,
            overflow: false,
        }
    }
}
impl Text {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.chars().count() > 4096
            || !(1.0..=512.0).contains(&self.font_size)
            || self.alignment > 8
            || !self.line_spacing.is_finite()
            || self.line_spacing <= 0.0
            || self.line_spacing > 16.0
            || self.color.iter().any(|v| !v.is_finite())
        {
            return Err("invalid Text values or text budget exceeded".into());
        }
        Ok(())
    }
}
