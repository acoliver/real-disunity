use super::{FontId, GlyphAtlas, UiError, UiLimits, atlas::AtlasBuilder, wrap};
use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use glam::{UVec2, Vec2};

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

/// A Unity `Text`. `font_size` is pixels per em, as in Unity. Glyphs are not clipped to the
/// rectangle; the overflow modes decide which lines exist.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub enabled: bool,
    pub font: FontId,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
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
            if !(c.is_ascii_graphic() || c == ' ' || c == '\n' || c == '\r') {
                return Err(UiError::UnsupportedShaping(c));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphLayout {
    pub character: char,
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
    pub lines: Vec<LineMetrics>,
    pub glyphs: Vec<GlyphLayout>,
    pub atlas: GlyphAtlas,
}
/// Unity font sizes are pixels per em; `ab_glyph` scales by ascent minus descent.
fn scale(font: &FontArc, font_size: f32) -> Result<PxScale, UiError> {
    let em = font.units_per_em().ok_or(UiError::InvalidFont)?;
    let scale = font_size * font.height_unscaled() / em;
    if !scale.is_finite() || scale <= 0. {
        return Err(UiError::InvalidFont);
    }
    Ok(PxScale::from(scale))
}

/// How many of `count` lines of height `line`, with `spacing` between baselines, fit in
/// `available`.
fn fitting(count: usize, line: f32, spacing: f32, available: f32) -> usize {
    let mut extent = line;
    let mut fits = 0;
    while fits < count && extent <= available {
        fits += 1;
        extent += spacing;
    }
    fits
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
    let spacing = (scaled.height() + scaled.line_gap()) * run.line_spacing;
    if run.vertical_overflow == VerticalOverflow::Truncate {
        lines.truncate(fitting(lines.len(), scaled.height(), spacing, size.y));
    }
    let height = scaled.height()
        + spacing
            * f32::from(
                u16::try_from(lines.len().saturating_sub(1)).map_err(|_| UiError::GlyphLimit)?,
            );
    let top = size.y - (size.y - height) * run.vertical.factor();
    if !top.is_finite() || !scaled.ascent().is_finite() {
        return Err(UiError::InvalidFont);
    }
    let mut atlas = AtlasBuilder::new(limits);
    let mut glyphs = Vec::new();
    let mut metrics = Vec::new();
    for (i, line) in lines.into_iter().enumerate() {
        let baseline = top
            - scaled.ascent()
            - f32::from(u16::try_from(i).map_err(|_| UiError::GlyphLimit)?) * spacing;
        metrics.push(LineMetrics {
            width: line.width,
            baseline,
        });
        let mut x = (size.x - line.width) * run.horizontal.factor();
        let mut previous = None;
        for c in line.characters {
            let id = scaled.glyph_id(c);
            x += previous.map_or(0., |p| scaled.kern(p, id));
            let glyph = id.with_scale_and_position(px, point(x, -baseline));
            if let Some(outlined) = font.outline_glyph(glyph)
                && let Some(glyph) = raster(&mut atlas, &outlined, c, Vec2::new(x, baseline))?
            {
                glyphs.push(glyph);
            }
            x += scaled.h_advance(id);
            previous = Some(id);
        }
    }
    Ok(TextLayout {
        lines: metrics,
        glyphs,
        atlas: atlas.finish(),
    })
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
        origin,
        glyph_id: glyph.glyph().id.0,
        rect_min: Vec2::new(min.x, -max.y),
        rect_max: Vec2::new(max.x, -min.y),
        atlas_min,
        atlas_size,
    }))
}
