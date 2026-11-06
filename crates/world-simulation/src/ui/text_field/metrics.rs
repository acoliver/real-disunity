use crate::ui::{FontBook, FontId, TextRun, UiError};
use ab_glyph::{Font, FontArc, ScaleFont};
use std::cell::{Cell, RefCell};

#[derive(Clone, Debug)]
pub(super) struct FieldMetrics {
    font: FontArc,
    id: FontId,
    scale: ab_glyph::PxScale,
    cached: RefCell<(String, Vec<f32>)>,
    pub offset: Cell<f32>,
}
impl PartialEq for FieldMetrics {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.scale == other.scale && self.offset == other.offset
    }
}
impl FieldMetrics {
    pub fn new(fonts: &FontBook, run: &TextRun) -> Result<Self, UiError> {
        let font = fonts.field_font(run.font)?;
        let scale = crate::ui::text::scale(&font, run.font_size)?;
        Ok(Self {
            font,
            id: run.font,
            scale,
            cached: RefCell::new((String::new(), vec![0.])),
            offset: Cell::new(0.),
        })
    }
    pub fn positions(&self, text: &str) -> Vec<f32> {
        let mut cached = self.cached.borrow_mut();
        if cached.0 != text {
            let scaled = self.font.as_scaled(self.scale);
            let mut positions = vec![0.];
            let mut previous = None;
            let mut x = 0.;
            for c in text.chars() {
                let id = scaled.glyph_id(c);
                x += previous.map_or(0., |p| scaled.kern(p, id));
                x += scaled.h_advance(id);
                positions.push(x);
                previous = Some(id);
            }
            *cached = (text.into(), positions);
        }
        cached.1.clone()
    }
    pub fn reveal(&self, positions: &[f32], cursor: usize, width: f32) -> f32 {
        let width = width.max(1.);
        let max = (positions[positions.len() - 1] + 1. - width).max(0.);
        let x = positions[cursor];
        let offset = self.offset.get().min(x).max(x + 1. - width).clamp(0., max);
        self.offset.set(offset);
        offset
    }
}
