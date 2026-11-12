//! Word wrapping with original UTF-8 byte provenance. Break spaces are consumed;
//! overlong words break between characters. Pair state resets at every line.
use super::{UiError, UiLimits, positioning::ScaledPairs};
use ab_glyph::{FontArc, GlyphId, PxScaleFont, ScaleFont};
use std::ops::Range;

pub(super) struct Line {
    pub characters: Vec<(usize, char)>,
    pub width: f32,
    pub paragraph_end: bool,
    pub source_range: Range<usize>,
    pub next_byte: usize,
}

type Scaled<'a> = PxScaleFont<&'a FontArc>;

struct Breaker<'a> {
    font: &'a Scaled<'a>,
    pairs: ScaledPairs<'a>,
    limit: Option<f32>,
    lines: Vec<Line>,
    current: Vec<(usize, char)>,
    width: f32,
    previous: Option<GlyphId>,
    start: usize,
}

impl Breaker<'_> {
    fn push(&mut self, byte: usize, character: char) -> Result<(), UiError> {
        let id = self.font.glyph_id(character);
        if id == GlyphId(0) {
            return Err(UiError::MissingGlyph(character));
        }
        self.width += self
            .previous
            .map_or(0., |p| self.pairs.x_advance(p.0, id.0))
            + self.font.h_advance(id);
        self.previous = Some(id);
        self.current.push((byte, character));
        while self.current.len() > 1 && self.limit.is_some_and(|limit| self.width > limit) {
            self.break_line();
        }
        Ok(())
    }

    fn break_line(&mut self) {
        let split = match self.current.iter().rposition(|&(_, c)| c == ' ') {
            Some(space) if space > 0 => space,
            _ => self.current.len() - 1,
        };
        let mut rest = self.current.split_off(split);
        let end = rest[0].0;
        let next = if rest[0].1 == ' ' {
            let (byte, character) = rest.remove(0);
            byte + character.len_utf8()
        } else {
            end
        };
        self.finish(false, end, next);
        (self.width, self.previous) = measure(self.font, self.pairs, &rest);
        self.current = rest;
    }

    fn finish(&mut self, paragraph_end: bool, end: usize, next_byte: usize) {
        let characters = std::mem::take(&mut self.current);
        let (width, _) = measure(self.font, self.pairs, &characters);
        self.lines.push(Line {
            characters,
            width,
            paragraph_end,
            source_range: self.start..end,
            next_byte,
        });
        self.start = next_byte;
        (self.width, self.previous) = (0., None);
    }
}

fn measure(
    font: &Scaled<'_>,
    pairs: ScaledPairs<'_>,
    characters: &[(usize, char)],
) -> (f32, Option<GlyphId>) {
    characters
        .iter()
        .fold((0., None), |(width, previous), &(_, c)| {
            let id = font.glyph_id(c);
            let kern = previous.map_or(0., |p: GlyphId| pairs.x_advance(p.0, id.0));
            (width + kern + font.h_advance(id), Some(id))
        })
}

pub(super) fn lines(
    font: &Scaled<'_>,
    pairs: ScaledPairs<'_>,
    text: &str,
    wrap_width: Option<f32>,
    limits: UiLimits,
) -> Result<Vec<Line>, UiError> {
    let mut breaker = Breaker {
        font,
        pairs,
        limit: wrap_width,
        lines: Vec::new(),
        current: Vec::new(),
        width: 0.,
        previous: None,
        start: 0,
    };
    for (count, (byte, character)) in text.char_indices().filter(|&(_, c)| c != '\r').enumerate() {
        if count >= limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        if character == '\n' {
            breaker.finish(true, byte, byte + 1);
        } else {
            breaker.push(byte, character)?;
        }
    }
    if !text.is_empty() {
        breaker.finish(false, text.len(), text.len());
    }
    if breaker.lines.iter().any(|line| !line.width.is_finite()) {
        return Err(UiError::InvalidFont);
    }
    Ok(breaker.lines)
}
