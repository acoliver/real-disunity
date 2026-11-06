//! Bounded resource reuse owned by the font book, independent of world mutations.
use super::{TextLayout, TextRun, UiError, UiLimits};
use glam::Vec2;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCacheStats {
    /// Successful layout misses, including empty text.
    pub layouts: u64,
    /// Successful misses producing nonempty glyph coverage.
    pub atlas_builds: u64,
}

struct Entry {
    run: TextRun,
    size: Vec2,
    layout: Arc<TextLayout>,
    characters: usize,
}
#[derive(Default)]
pub(super) struct TextCache {
    entries: VecDeque<Entry>,
    characters: usize,
    pixels: usize,
    pub stats: TextCacheStats,
}
impl TextCache {
    pub fn layout(
        &mut self,
        run: &TextRun,
        size: Vec2,
        limits: UiLimits,
        build: impl FnOnce() -> Result<TextLayout, UiError>,
    ) -> Result<Arc<TextLayout>, UiError> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|e| e.run == *run && e.size == size)
        {
            return Ok(entry.layout.clone());
        }
        let layout = Arc::new(build()?);
        self.stats.layouts += 1;
        self.stats.atlas_builds += u64::from(!layout.atlas.alpha.is_empty());
        let characters = run.text.chars().count().max(1);
        let pixels = layout.atlas.alpha.len();
        while self.characters + characters > limits.max_glyphs
            || self.pixels + pixels > limits.max_atlas_pixels
        {
            let entry = self.entries.pop_front().expect("layout fits cache budgets");
            self.characters -= entry.characters;
            self.pixels -= entry.layout.atlas.alpha.len();
        }
        self.characters += characters;
        self.pixels += pixels;
        self.entries.push_back(Entry {
            run: run.clone(),
            size,
            layout: layout.clone(),
            characters,
        });
        Ok(layout)
    }
}
