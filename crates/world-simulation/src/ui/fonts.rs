use super::text_cache::{TextCache, TextCacheStats};
use super::{FontId, TextLayout, TextRun, UiError};
use ab_glyph::FontArc;
use glam::Vec2;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
static NEXT_FONT: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiLimits {
    pub max_fonts: u32,
    pub max_font_bytes: usize,
    pub max_glyphs: usize,
    pub max_atlas_side: u32,
    pub max_atlas_pixels: usize,
}
impl Default for UiLimits {
    fn default() -> Self {
        Self {
            max_fonts: 16,
            max_font_bytes: 8 * 1024 * 1024,
            max_glyphs: 4096,
            max_atlas_side: 2048,
            max_atlas_pixels: 4 * 1024 * 1024,
        }
    }
}

pub struct FontBook {
    fonts: BTreeMap<FontId, FontArc>,
    bytes: usize,
    cache: Mutex<TextCache>,
    pub(super) limits: UiLimits,
}
impl FontBook {
    pub fn limits(&self) -> UiLimits {
        self.limits
    }
    pub fn new(limits: UiLimits) -> Result<Self, UiError> {
        if limits.max_fonts == 0
            || limits.max_font_bytes == 0
            || limits.max_glyphs == 0
            || limits.max_glyphs > 65536
            || limits.max_atlas_side == 0
            || limits.max_atlas_side > 16384
            || limits.max_atlas_pixels == 0
            || limits.max_atlas_pixels > 64 * 1024 * 1024
        {
            return Err(UiError::InvalidText);
        }
        Ok(Self {
            fonts: BTreeMap::new(),
            bytes: 0,
            cache: Mutex::default(),
            limits,
        })
    }
    /// Takes owned SFNT bytes. Parsing and limits are checked before publication.
    pub fn register(&mut self, bytes: Vec<u8>) -> Result<FontId, UiError> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(UiError::FontLimit)?;
        let index = u32::try_from(self.fonts.len()).map_err(|_| UiError::FontLimit)?;
        if index >= self.limits.max_fonts || total > self.limits.max_font_bytes {
            return Err(UiError::FontLimit);
        }
        super::font_data::validate(&bytes)?;
        let font = FontArc::try_from_vec(bytes).map_err(|_| UiError::InvalidFont)?;
        let id = FontId(
            NEXT_FONT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
                .map_err(|_| UiError::FontLimit)?,
        );
        self.fonts.insert(id, font);
        self.bytes = total;
        Ok(id)
    }
    pub(super) fn validate_single_line(&self, run: &TextRun) -> Result<(), UiError> {
        use ab_glyph::Font;
        run.validate()?;
        let font = self
            .fonts
            .get(&run.font)
            .ok_or(UiError::MissingFont(run.font))?;
        for c in run.text.chars() {
            if !c.is_ascii() || c == '\n' || c == '\r' {
                return Err(UiError::UnsupportedShaping(c));
            }
            if font.glyph_id(c).0 == 0 {
                return Err(UiError::MissingGlyph(c));
            }
        }
        Ok(())
    }
    pub(super) fn text_positions(&self, run: &TextRun) -> Result<Vec<f32>, UiError> {
        use ab_glyph::{Font, ScaleFont};
        self.validate_single_line(run)?;
        let font = &self.fonts[&run.font];
        let scaled = font.as_scaled(super::text::scale(font, run.font_size)?);
        let mut positions = vec![0.];
        let mut previous = None;
        let mut x = 0.;
        for c in run.text.chars() {
            let id = scaled.glyph_id(c);
            x += previous.map_or(0., |p| scaled.kern(p, id));
            x += scaled.h_advance(id);
            positions.push(x);
            previous = Some(id);
        }
        Ok(positions)
    }
    pub fn cache_stats(&self) -> TextCacheStats {
        self.cache.lock().expect("text cache lock").stats
    }
    /// Reuses immutable glyphs and coverage until bounded cache eviction.
    pub fn layout(&self, run: &TextRun, size: Vec2) -> Result<Arc<TextLayout>, UiError> {
        run.validate()?;
        if !size.is_finite() || size.min_element() < 0. {
            return Err(UiError::InvalidRect);
        }
        if run.text.chars().count() > self.limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        let font = self
            .fonts
            .get(&run.font)
            .ok_or(UiError::MissingFont(run.font))?;
        self.cache
            .lock()
            .expect("text cache lock")
            .layout(run, size, self.limits, || {
                super::text::layout(font, run, size, self.limits)
            })
    }
}
