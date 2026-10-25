use super::{FontId, GlyphAtlas, UiError, UiLimits, atlas::AtlasBuilder};
use ab_glyph::{Font, FontArc, GlyphId, ScaleFont, point};
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
    pub wrap: bool,
    /// When false, raster output is clipped to the rectangle.
    pub overflow: bool,
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
            wrap: false,
            overflow: false,
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
struct Line {
    characters: Vec<char>,
    width: f32,
}

fn lines(
    font: &FontArc,
    run: &TextRun,
    size: Vec2,
    limits: UiLimits,
) -> Result<Vec<Line>, UiError> {
    let scaled = font.as_scaled(run.font_size);
    let mut lines = Vec::new();
    let mut line = Line {
        characters: Vec::new(),
        width: 0.,
    };
    let mut previous = None;
    let mut count = 0;
    for c in run.text.chars() {
        if c == '\r' {
            continue;
        }
        count += 1;
        if count > limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        if c == '\n' {
            lines.push(line);
            line = Line {
                characters: Vec::new(),
                width: 0.,
            };
            previous = None;
            continue;
        }
        let id = scaled.glyph_id(c);
        if id == GlyphId(0) {
            return Err(UiError::MissingGlyph(c));
        }
        let kern = previous.map_or(0., |p| scaled.kern(p, id));
        let advance = scaled.h_advance(id);
        if run.wrap && !line.characters.is_empty() && line.width + kern + advance > size.x {
            lines.push(line);
            line = Line {
                characters: Vec::new(),
                width: 0.,
            };
            previous = None;
        }
        line.width += previous.map_or(0., |p| scaled.kern(p, id)) + advance;
        line.characters.push(c);
        previous = Some(id);
    }
    if !run.text.is_empty() {
        lines.push(line);
    }
    if lines.iter().any(|line| !line.width.is_finite()) {
        return Err(UiError::InvalidFont);
    }
    Ok(lines)
}

pub(super) fn layout(
    font: &FontArc,
    run: &TextRun,
    size: Vec2,
    limits: UiLimits,
) -> Result<TextLayout, UiError> {
    let lines = lines(font, run, size, limits)?;
    let scaled = font.as_scaled(run.font_size);
    let spacing = (scaled.height() + scaled.line_gap()) * run.line_spacing;
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
            let glyph = id.with_scale_and_position(run.font_size, point(x, -baseline));
            if let Some(outlined) = font.outline_glyph(glyph)
                && let Some(glyph) =
                    raster(&mut atlas, &outlined, c, size, run, Vec2::new(x, baseline))?
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
    size: Vec2,
    run: &TextRun,
    origin: Vec2,
) -> Result<Option<GlyphLayout>, UiError> {
    let bounds = glyph.px_bounds();
    let mut min = Vec2::new(bounds.min.x, bounds.min.y);
    let mut max = Vec2::new(bounds.max.x, bounds.max.y);
    if !run.overflow {
        min = min.max(Vec2::new(0., -size.y).ceil());
        max = max.min(Vec2::new(size.x, 0.).floor());
    }
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
