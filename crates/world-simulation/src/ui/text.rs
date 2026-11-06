use super::{FontId, GlyphAtlas, UiError, UiLimits, atlas::AtlasBuilder, wrap};
use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use glam::{UVec2, Vec2};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}
impl HorizontalAlign {
    fn factor(self) -> f32 {
        match self {
            Self::Left => 0.,
            Self::Center => 0.5,
            Self::Right => 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}
impl VerticalAlign {
    fn factor(self) -> f32 {
        match self {
            Self::Top => 0.,
            Self::Middle => 0.5,
            Self::Bottom => 1.,
        }
    }
}

/// Unity's `HorizontalWrapMode`: wrap lines at word boundaries inside the rectangle, or keep
/// each line whole however wide it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HorizontalOverflow {
    #[default]
    Wrap,
    Overflow,
}
/// Unity's `VerticalWrapMode`: drop the lines that do not fit the rectangle's height, or keep
/// them all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalOverflow {
    #[default]
    Truncate,
    Overflow,
}

/// Caller-chosen identity, unique among the link spans of one text run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UiLinkId(pub u32);

/// A styled UTF-8 byte range. Ranges must be ordered, disjoint and on character boundaries.
/// Colour multiplies the run tint. No synthetic weight or font substitution is applied.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub range: Range<usize>,
    pub color: [f32; 4],
    pub link: Option<UiLinkId>,
}

/// Text with word wrapping. `font_size` is pixels per em. Contributions clip to the rectangle;
/// overflow modes decide which lines are laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub enabled: bool,
    pub font: FontId,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub spans: Vec<TextSpan>,
    /// Baseline distance in logical pixels; `None` uses font metrics and `line_spacing`.
    pub line_height: Option<f32>,
    /// Extra logical pixels after each explicit newline, excluding the last line.
    pub paragraph_spacing: f32,
    pub horizontal: HorizontalAlign,
    pub vertical: VerticalAlign,
    pub line_spacing: f32,
    pub horizontal_overflow: HorizontalOverflow,
    pub vertical_overflow: VerticalOverflow,
    /// Unity enables rich text by default. Text without markup lays out the same either way, so
    /// only markup in rich text is rejected.
    pub rich_text: bool,
}
impl TextRun {
    pub fn new(font: FontId) -> Self {
        Self {
            enabled: true,
            font,
            text: String::new(),
            font_size: 14.,
            color: [1.; 4],
            spans: Vec::new(),
            line_height: None,
            paragraph_spacing: 0.,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            line_spacing: 1.,
            horizontal_overflow: HorizontalOverflow::Wrap,
            vertical_overflow: VerticalOverflow::Truncate,
            rich_text: true,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), UiError> {
        if !(1. ..=512.).contains(&self.font_size)
            || !(0.01..=16.).contains(&self.line_spacing)
            || self.line_height.is_some_and(|h| !h.is_finite() || h <= 0.)
            || !self.paragraph_spacing.is_finite()
            || self.paragraph_spacing < 0.
            || self.color.iter().any(|v| !(0. ..=1.).contains(v))
        {
            return Err(UiError::InvalidText);
        }
        if self.text.len() > 65536 {
            return Err(UiError::GlyphLimit);
        }
        if self.rich_text && self.text.contains('<') {
            return Err(UiError::UnsupportedRichText);
        }
        for c in self.text.chars() {
            if !(c.is_ascii_graphic()
                || c == ' '
                || c == '\n'
                || c == '\r'
                || ('\u{a0}'..='\u{ff}').contains(&c))
            {
                return Err(UiError::UnsupportedShaping(c));
            }
        }
        self.validate_spans()
    }
    fn validate_spans(&self) -> Result<(), UiError> {
        let mut end = 0;
        let mut links = std::collections::BTreeSet::new();
        for span in &self.spans {
            if span.range.start < end
                || span.range.start >= span.range.end
                || self.text.get(span.range.clone()).is_none()
                || span.color.iter().any(|v| !(0. ..=1.).contains(v))
                || span.link.is_some_and(|id| !links.insert(id))
            {
                return Err(UiError::InvalidText);
            }
            end = span.range.end;
        }
        Ok(())
    }
    fn span(&self, byte: usize) -> Option<&TextSpan> {
        let index = self.spans.partition_point(|s| s.range.end <= byte);
        self.spans.get(index).filter(|s| s.range.contains(&byte))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphLayout {
    pub character: char,
    pub color: [f32; 4],
    pub link: Option<UiLinkId>,
    pub glyph_id: u16,
    pub origin: Vec2,
    /// Local X-right/Y-up coordinates measured from rectangle bottom-left.
    pub rect_min: Vec2,
    pub rect_max: Vec2,
    /// Atlas pixel rectangle, top-left origin.
    pub atlas_min: UVec2,
    pub atlas_size: UVec2,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LineMetrics {
    pub width: f32,
    pub baseline: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// Full pre-truncation extent, suitable for a scroll content's fixed height.
    pub content_height: f32,
    pub lines: Vec<LineMetrics>,
    pub glyphs: Vec<GlyphLayout>,
    pub atlas: GlyphAtlas,
}
/// Unity font sizes are pixels per em; `ab_glyph` scales by ascent minus descent.
pub(super) fn scale(font: &FontArc, font_size: f32) -> Result<PxScale, UiError> {
    let em = font.units_per_em().ok_or(UiError::InvalidFont)?;
    let scale = font_size * font.height_unscaled() / em;
    if !scale.is_finite() || scale <= 0. {
        return Err(UiError::InvalidFont);
    }
    Ok(PxScale::from(scale))
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

pub(super) fn layout(
    font: &FontArc,
    run: &TextRun,
    size: Vec2,
    limits: UiLimits,
) -> Result<TextLayout, UiError> {
    let px = scale(font, run.font_size)?;
    let scaled = font.as_scaled(px);
    let wrap_width = (run.horizontal_overflow == HorizontalOverflow::Wrap).then_some(size.x);
    let mut lines = wrap::lines(&scaled, &run.text, wrap_width, limits)?;
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
    let mut atlas = AtlasBuilder::new(limits);
    let mut glyphs = Vec::new();
    let mut metrics = Vec::new();
    for (line, offset) in lines.into_iter().zip(offsets) {
        let baseline = top - scaled.ascent() - offset;
        metrics.push(LineMetrics {
            width: line.width,
            baseline,
        });
        layout_line(
            &mut atlas,
            &mut glyphs,
            &scaled,
            run,
            &line,
            Vec2::new(size.x, baseline),
        )?;
    }
    Ok(TextLayout {
        content_height,
        lines: metrics,
        glyphs,
        atlas: atlas.finish(),
    })
}

fn layout_line(
    atlas: &mut AtlasBuilder,
    glyphs: &mut Vec<GlyphLayout>,
    font: &ab_glyph::PxScaleFont<&FontArc>,
    run: &TextRun,
    line: &wrap::Line,
    extent: Vec2,
) -> Result<(), UiError> {
    let mut x = (extent.x - line.width) * run.horizontal.factor();
    let mut previous = None;
    for &(byte, c) in &line.characters {
        let id = font.glyph_id(c);
        x += previous.map_or(0., |p| font.kern(p, id));
        let glyph = id.with_scale_and_position(font.scale(), point(x, -extent.y));
        if let Some(outlined) = font.font.outline_glyph(glyph)
            && let Some(mut glyph) = raster(atlas, &outlined, c, Vec2::new(x, extent.y))?
        {
            if let Some(span) = run.span(byte) {
                glyph.color = span.color;
                glyph.link = span.link;
            }
            glyphs.push(glyph);
        }
        x += font.h_advance(id);
        previous = Some(id);
    }
    Ok(())
}

fn raster(
    atlas: &mut AtlasBuilder,
    glyph: &ab_glyph::OutlinedGlyph,
    character: char,
    origin: Vec2,
) -> Result<Option<GlyphLayout>, UiError> {
    let bounds = glyph.px_bounds();
    let min = Vec2::new(bounds.min.x, bounds.min.y);
    let max = Vec2::new(bounds.max.x, bounds.max.y);
    if max.cmple(min).any() {
        return Ok(None);
    }
    let (atlas_min, atlas_size) = atlas.insert(glyph, min, max)?;
    Ok(Some(GlyphLayout {
        character,
        color: [1.; 4],
        link: None,
        origin,
        glyph_id: glyph.glyph().id.0,
        rect_min: Vec2::new(min.x, -max.y),
        rect_max: Vec2::new(max.x, -min.y),
        atlas_min,
        atlas_size,
    }))
}
