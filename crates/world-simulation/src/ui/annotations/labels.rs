use super::{AnnotationPoint, MAX_ANNOTATION_VERTICES, draw, point};
use crate::presentation::CoverageTexture;
use crate::ui::{
    FontBook, HorizontalAlign, HorizontalOverflow, TextRun, UiError, UiLimits, VerticalAlign,
    VerticalOverflow,
    presentation::{UiDrawKind, UiPresentation},
};
use glam::Vec2;
use worldrunner_camera::ResolvedView;

#[derive(Default)]
pub(super) struct Budget {
    vertices: usize,
    glyphs: usize,
    pixels: usize,
}
impl Budget {
    pub(super) fn vertices(&mut self, count: usize) -> Result<(), UiError> {
        self.vertices += count;
        if self.vertices > MAX_ANNOTATION_VERTICES {
            return Err(UiError::AnnotationLimit);
        }
        Ok(())
    }
    fn text(&mut self, glyphs: usize, pixels: usize, limits: UiLimits) -> Result<(), UiError> {
        self.glyphs += glyphs;
        self.pixels += pixels;
        if self.glyphs > limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        if self.pixels > limits.max_atlas_pixels {
            return Err(UiError::AtlasLimit);
        }
        Ok(())
    }
}

pub(super) fn extract(
    plan: &mut UiPresentation,
    budget: &mut Budget,
    fonts: &FontBook,
    view: &ResolvedView,
    label: (AnnotationPoint, &TextRun, Vec2),
) -> Result<(), UiError> {
    let (anchor, run, alignment) = label;
    if !alignment.is_finite() || alignment.min_element() < 0. || alignment.max_element() > 1. {
        return Err(UiError::InvalidAnnotation);
    }
    run.validate()?;
    let Some(anchor) = point(anchor, view)? else {
        return Ok(());
    };
    if !run.enabled {
        return Ok(());
    }
    let mut run = run.clone();
    run.horizontal = HorizontalAlign::Left;
    run.vertical = VerticalAlign::Bottom;
    run.horizontal_overflow = HorizontalOverflow::Overflow;
    run.vertical_overflow = VerticalOverflow::Overflow;
    let text = fonts.layout(&run, Vec2::splat(16384.))?;
    budget.text(
        run.text.chars().filter(|c| *c != '\r').count(),
        text.atlas.alpha.len(),
        fonts.limits(),
    )?;
    budget.vertices(text.glyphs.len() * 6)?;
    let Some(first) = text.glyphs.first() else {
        return Ok(());
    };
    let (min, max) = text
        .glyphs
        .iter()
        .fold((first.rect_min, first.rect_max), |(min, max), g| {
            (min.min(g.rect_min), max.max(g.rect_max))
        });
    let shift = anchor - min - alignment * (max - min);
    let size = text.atlas.size.as_vec2();
    let identity = plan.coverage.len() as u64;
    for glyph in text.glyphs {
        plan.draws
            .push(glyph_draw(&glyph, shift, size, run.color, identity));
    }
    plan.coverage.insert(
        identity,
        CoverageTexture {
            width: text.atlas.size.x,
            height: text.atlas.size.y,
            bytes: text.atlas.alpha.into_boxed_slice(),
        },
    );
    Ok(())
}

fn glyph_draw(
    glyph: &crate::ui::GlyphLayout,
    shift: Vec2,
    size: Vec2,
    colour: [f32; 4],
    identity: u64,
) -> crate::ui::presentation::UiDraw {
    let min = glyph.rect_min + shift;
    let max = glyph.rect_max + shift;
    let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
        .map(|p| p.extend(0.).to_array());
    let mut glyph_draw = draw(corners, colour);
    glyph_draw.kind = UiDrawKind::Text;
    glyph_draw.triangle_uv = None;
    glyph_draw.texture = Some(identity);
    let uv_min = glyph.atlas_min.as_vec2() / size;
    let uv_max = (glyph.atlas_min + glyph.atlas_size).as_vec2() / size;
    glyph_draw.uv_rect = [uv_min.x, uv_min.y, uv_max.x, uv_max.y];
    glyph_draw
}
