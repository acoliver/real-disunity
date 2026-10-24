use crate::{DisunityWorld, components::Fields};
use ab_glyph::{FontArc, FontVec};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) mod fields;
mod layout;
mod text;

#[derive(Clone)]
pub(crate) struct Ui {
    pub viewport: [u32; 2],
    fonts: BTreeMap<String, (FontArc, usize)>,
}
impl Default for Ui {
    fn default() -> Self {
        Self {
            viewport: [1920, 1080],
            fonts: BTreeMap::new(),
        }
    }
}
impl Ui {
    fn register_font(&mut self, name: String, data: String) -> Result<Value, String> {
        if name.is_empty() || name.len() > 128 || data.len() > 1_048_000 {
            return Err("font name/data exceeds budget".into());
        }
        let bytes = STANDARD
            .decode(data)
            .map_err(|e| format!("invalid font base64: {e}"))?;
        let len = bytes.len();
        let old = self.fonts.get(&name).map_or(0, |v| v.1);
        let total: usize = self.fonts.values().map(|v| v.1).sum();
        if (self.fonts.len() >= 16 && old == 0) || total - old + len > 8 * 1024 * 1024 {
            return Err("world font budget exceeded".into());
        }
        let font = FontVec::try_from_vec(bytes).map_err(|e| format!("invalid font: {e}"))?;
        self.fonts.insert(name.clone(), (FontArc::new(font), len));
        Ok(json!({"name":name,"bytes":len}))
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum UiRequest {
    #[serde(rename = "font.register")]
    Font { name: String, data_base64: String },
    #[serde(rename = "ui.configure")]
    Configure { viewport: [u32; 2] },
}
impl UiRequest {
    pub(crate) fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Font { name, data_base64 } => w.ui.register_font(name, data_base64),
            Self::Configure { viewport } => {
                if viewport.iter().any(|v| *v == 0 || *v > 16384) {
                    return Err("viewport dimensions must be in 1..16384".into());
                }
                w.ui.viewport = viewport;
                Ok(json!({"viewport":viewport}))
            }
        }
    }
}
impl DisunityWorld {
    pub(crate) fn ui_layout(&self) -> Result<Value, String> {
        let mut rects = Vec::new();
        let mut texts = Vec::new();
        let mut pixels = 0;
        let mut glyphs = 0;
        for c in self.components.iter().flatten() {
            if !c.enabled || !self.active(c.object) || !self.canvas_visible(c.object) {
                continue;
            }
            if matches!(c.fields, Fields::Rect(_)) {
                rects.push(self.rect_snapshot(c.object)?);
            }
            if let Fields::Text(t) = &c.fields {
                texts.push(self.text_snapshot(c, t, &mut pixels, &mut glyphs)?);
            }
        }
        Ok(json!({"viewport":self.ui.viewport,"rects":rects,"texts":texts}))
    }
    fn canvas_visible(&self, object: u64) -> bool {
        let mut current = Some(object);
        while let Some(h) = current {
            if self
                .components
                .iter()
                .flatten()
                .any(|c| c.object == h && matches!(c.fields, Fields::Canvas(_)) && !c.enabled)
            {
                return false;
            }
            current = self.metadata[&h].parent;
        }
        true
    }
}
