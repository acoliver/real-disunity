//! Word wrapping with original UTF-8 byte provenance. Break spaces are consumed;
//! overlong words break between characters. Pair state resets at every line.
use super::{UiError, UiLimits, positioning::ScaledPairs};
use ab_glyph::{FontArc, GlyphId, PxScaleFont, ScaleFont};
use std::ops::Range;

/// Caller-supplied inclusive Unicode ranges that permit a break before a character.
/// ASCII space is always a candidate. This is boundary analysis, independent of shaping.
#[derive(Clone, Debug, Default)]
pub struct LineBreakCandidates {
    ranges: Vec<(u32, u32)>,
}

impl LineBreakCandidates {
    pub fn new(ranges: Vec<(u32, u32)>) -> Result<Self, UiError> {
        if ranges.len() > 32768
            || ranges
                .iter()
                .any(|&(start, end)| start > end || end > 0x0010_ffff)
        {
            return Err(UiError::InvalidText);
        }
        Ok(Self { ranges })
    }

    /// Returns the next candidate strictly after `start`, at the break character's byte.
    /// Forbidden sets exclude boundaries before/after their respective characters.
    /// Nonempty remaining text ends with a terminal boundary at `text.len()`, then None.
    /// All offsets are zero-based UTF-8 byte boundaries; invalid offsets return `InvalidText`.
    pub fn next(
        &self,
        text: &str,
        start: usize,
        forbidden_start: &str,
        forbidden_end: &str,
    ) -> Result<Option<usize>, UiError> {
        let remaining = text.get(start..).ok_or(UiError::InvalidText)?;
        let mut previous = None;
        for (byte, character) in remaining.char_indices() {
            let candidate = character == ' '
                || self
                    .ranges
                    .iter()
                    .any(|&(a, b)| (a..=b).contains(&u32::from(character)));
            if byte > 0
                && candidate
                && !forbidden_start.contains(character)
                && !previous.is_some_and(|c| forbidden_end.contains(c))
            {
                return Ok(Some(start + byte));
            }
            previous = Some(character);
        }
        Ok((!remaining.is_empty()).then_some(text.len()))
    }
}

/// Classifies only the first/last character against caller-owned line-boundary policy.
pub fn line_break_info(text: &str, forbidden_start: &str, forbidden_end: &str) -> (bool, bool) {
    (
        text.chars()
            .next()
            .is_some_and(|c| forbidden_start.contains(c)),
        text.chars()
            .next_back()
            .is_some_and(|c| forbidden_end.contains(c)),
    )
}

/// Advances a checked UTF-8 byte boundary by a signed character count, clamped to the text.
pub fn advance_text(text: &str, start: usize, count: i64) -> Result<usize, UiError> {
    let remaining = text.get(start..).ok_or(UiError::InvalidText)?;
    let steps = usize::try_from(count.unsigned_abs()).unwrap_or(usize::MAX);
    if count >= 0 {
        Ok(remaining
            .char_indices()
            .nth(steps)
            .map_or(text.len(), |(byte, _)| start + byte))
    } else {
        Ok(text[..start]
            .char_indices()
            .rev()
            .take(steps)
            .last()
            .map_or(start, |(byte, _)| byte))
    }
}

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
