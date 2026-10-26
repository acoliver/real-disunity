//! Line breaking as Unity's text generator does it. A line that grows past the wrap width breaks
//! before its last word, consuming the space at the break. A word wider than the line breaks
//! between characters.
use super::{UiError, UiLimits};
use ab_glyph::{FontArc, GlyphId, PxScaleFont, ScaleFont};

pub(super) struct Line {
    pub characters: Vec<char>,
    pub width: f32,
}

type Scaled<'a> = PxScaleFont<&'a FontArc>;

struct Breaker<'a> {
    font: &'a Scaled<'a>,
    limit: Option<f32>,
    lines: Vec<Line>,
    current: Vec<char>,
    width: f32,
    previous: Option<GlyphId>,
}

impl Breaker<'_> {
    fn push(&mut self, character: char) -> Result<(), UiError> {
        let id = self.font.glyph_id(character);
        if id == GlyphId(0) {
            return Err(UiError::MissingGlyph(character));
        }
        self.width += self.previous.map_or(0., |p| self.font.kern(p, id)) + self.font.h_advance(id);
        self.previous = Some(id);
        self.current.push(character);
        while self.overflows() {
            self.break_line();
        }
        Ok(())
    }

    fn overflows(&self) -> bool {
        self.current.len() > 1 && self.limit.is_some_and(|limit| self.width > limit)
    }

    fn break_line(&mut self) {
        let split = match self.current.iter().rposition(|&c| c == ' ') {
            Some(space) if space > 0 => space,
            _ => self.current.len() - 1,
        };
        let mut rest = self.current.split_off(split);
        if rest.first() == Some(&' ') {
            rest.remove(0);
        }
        self.finish();
        (self.width, self.previous) = measure(self.font, &rest);
        self.current = rest;
    }

    fn finish(&mut self) {
        let characters = std::mem::take(&mut self.current);
        let (width, _) = measure(self.font, &characters);
        self.lines.push(Line { characters, width });
        (self.width, self.previous) = (0., None);
    }
}

/// Width of `characters` with kerning, and the last glyph for kerning what follows. Callers
/// have already rejected characters the font lacks.
fn measure(font: &Scaled<'_>, characters: &[char]) -> (f32, Option<GlyphId>) {
    characters.iter().fold((0., None), |(width, previous), &c| {
        let id = font.glyph_id(c);
        let kern = previous.map_or(0., |p| font.kern(p, id));
        (width + kern + font.h_advance(id), Some(id))
    })
}

/// Splits `text` into lines at newlines, and at `wrap_width` when it is set.
pub(super) fn lines(
    font: &Scaled<'_>,
    text: &str,
    wrap_width: Option<f32>,
    limits: UiLimits,
) -> Result<Vec<Line>, UiError> {
    let mut breaker = Breaker {
        font,
        limit: wrap_width,
        lines: Vec::new(),
        current: Vec::new(),
        width: 0.,
        previous: None,
    };
    for (count, character) in text.chars().filter(|&c| c != '\r').enumerate() {
        if count >= limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        if character == '\n' {
            breaker.finish();
        } else {
            breaker.push(character)?;
        }
    }
    if !text.is_empty() {
        breaker.finish();
    }
    if breaker.lines.iter().any(|line| !line.width.is_finite()) {
        return Err(UiError::InvalidFont);
    }
    Ok(breaker.lines)
}
