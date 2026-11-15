//! The shared, atlas-free line-placement stage for measurement and raster layout.
use super::positioning::ScaledPairs;
use super::{HorizontalOverflow, LineMetrics, TextRun, UiError, UiLimits, VerticalOverflow, wrap};
use ab_glyph::{FontArc, PxScaleFont, ScaleFont};
use glam::Vec2;

/// Computes all prefix advances once, including pairs and explicit paragraph boundaries.
pub(super) fn fit_prefix(
    registered: &super::fonts::RegisteredFont,
    run: &TextRun,
    width: f32,
) -> Result<usize, UiError> {
    use ab_glyph::{Font, GlyphId};
    let scaled = registered
        .font
        .as_scaled(super::text::scale(&registered.font, run.font_size)?);
    let face = ttf_parser::Face::parse(registered.font.font_data(), 0)
        .map_err(|_| UiError::InvalidFont)?;
    let pairs = registered.positioning.scaled(&face, run.font_size);
    let mut previous: Option<GlyphId> = None;
    let (mut current, mut completed) = (0_f32, 0_f32);
    let mut end = 0;
    for (byte, character) in run.text.char_indices() {
        match character {
            '\r' => (),
            '\n' => {
                completed = completed.max(current);
                current = 0.;
                previous = None;
            }
            _ => {
                let id = scaled.glyph_id(character);
                if id == GlyphId(0) {
                    return Err(UiError::MissingGlyph(character));
                }
                current += previous.map_or(0., |p| pairs.x_advance(p.0, id.0));
                current += scaled.h_advance(id);
                previous = Some(id);
            }
        }
        if !current.is_finite() {
            return Err(UiError::InvalidFont);
        }
        if completed.max(current) <= width {
            end = byte + character.len_utf8();
        }
    }
    Ok(end)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMeasurement {
    /// Maximum line advance, including unconsumed spaces, without ink or effects.
    pub width: f32,
    /// Full metric height, including blank/trailing lines and paragraph spacing.
    pub height: f32,
    pub lines: Vec<LineMetrics>,
}

pub(super) struct PlacedLines {
    pub content_height: f32,
    pub lines: Vec<wrap::Line>,
    pub metrics: Vec<LineMetrics>,
}

fn offsets(lines: &[wrap::Line], spacing: f32, paragraph: f32) -> Vec<f32> {
    let mut offset = 0.;
    let mut result = Vec::with_capacity(lines.len());
    for line in lines {
        result.push(offset);
        offset += spacing + if line.paragraph_end { paragraph } else { 0. };
    }
    result
}

pub(super) fn place(
    scaled: &PxScaleFont<&FontArc>,
    pairs: ScaledPairs<'_>,
    run: &TextRun,
    size: Vec2,
    limits: UiLimits,
) -> Result<PlacedLines, UiError> {
    let wrap_width = (run.horizontal_overflow == HorizontalOverflow::Wrap).then_some(size.x);
    let mut lines = wrap::lines(scaled, pairs, &run.text, wrap_width, limits)?;
    let spacing = run
        .line_height
        .unwrap_or((scaled.height() + scaled.line_gap()) * run.line_spacing);
    let offsets = offsets(&lines, spacing, run.paragraph_spacing);
    let content_height = offsets.last().map_or(0., |last| last + scaled.height());
    if run.vertical_overflow == VerticalOverflow::Truncate {
        lines.truncate(
            offsets
                .iter()
                .take_while(|&&y| y + scaled.height() <= size.y)
                .count(),
        );
    }
    let height = lines
        .len()
        .checked_sub(1)
        .map_or(0., |i| offsets[i] + scaled.height());
    let top = size.y - (size.y - height) * run.vertical.factor();
    if !top.is_finite() || !content_height.is_finite() || !scaled.ascent().is_finite() {
        return Err(UiError::InvalidFont);
    }
    let metrics = lines
        .iter()
        .zip(offsets)
        .map(|(line, offset)| LineMetrics {
            width: line.width,
            baseline: top - scaled.ascent() - offset,
            source_range: line.source_range.clone(),
            next_byte: line.next_byte,
        })
        .collect();
    Ok(PlacedLines {
        content_height,
        lines,
        metrics,
    })
}
