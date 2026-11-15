use super::super::{GlyphLayout, HorizontalOverflow, TextMeasurement, TextRun, VerticalOverflow};
use super::{
    UiDrawKind, UiDrawList, UiError, UiTextEffect, UiTextPlacement, check_color, check_transform,
    multiply, rotate,
};
use crate::presentation::CoverageTexture;
use glam::Vec2;
use std::sync::Arc;

impl UiDrawList<'_> {
    /// Full metric height and maximum advance. None disables horizontal wrapping.
    pub fn measure_text(
        &self,
        run: &TextRun,
        wrap_width: Option<f32>,
    ) -> Result<TextMeasurement, UiError> {
        if wrap_width.is_some_and(|w| !w.is_finite() || w < 0.) {
            return Err(UiError::InvalidRect);
        }
        let mut run = run.clone();
        run.horizontal_overflow = if wrap_width.is_some() {
            HorizontalOverflow::Wrap
        } else {
            HorizontalOverflow::Overflow
        };
        run.vertical_overflow = VerticalOverflow::Overflow;
        self.fonts
            .measure(&run, Vec2::new(wrap_width.unwrap_or(0.), 0.))
    }
    pub fn text(
        &mut self,
        run: &TextRun,
        placement: UiTextPlacement,
        effect: UiTextEffect,
    ) -> Result<(), UiError> {
        placement.destination.validate()?;
        validate_effect(effect)?;
        if self.hidden(placement.destination) {
            return Ok(());
        }
        check_transform(placement.rotation_radians, placement.pivot)?;
        let scale = placement.destination.size / placement.layout_size;
        if !placement.layout_size.is_finite()
            || placement.layout_size.min_element() <= 0.
            || !scale.is_finite()
            || scale.min_element() <= 0.
        {
            return Err(UiError::InvalidRect);
        }
        if !run.enabled {
            return Ok(());
        }
        let characters = self
            .characters
            .checked_add(run.text.chars().count())
            .ok_or(UiError::GlyphLimit)?;
        if characters > self.limits.max_characters {
            return Err(UiError::GlyphLimit);
        }
        let layout = self.fonts.layout(run, placement.layout_size)?;
        let passes = if effect == UiTextEffect::None { 1 } else { 2 };
        self.check_count(
            layout
                .glyphs
                .len()
                .checked_mul(passes)
                .ok_or(UiError::DrawLimit)?,
        )?;
        let existing = self
            .atlases
            .iter()
            .position(|bytes| Arc::ptr_eq(bytes, &layout.atlas.alpha));
        let references_atlas = !layout.glyphs.is_empty();
        let added_pixels = if references_atlas && existing.is_none() {
            layout.atlas.alpha.len()
        } else {
            0
        };
        let pixels = self
            .atlas_pixels
            .checked_add(added_pixels)
            .ok_or(UiError::AtlasLimit)?;
        if pixels > self.limits.max_atlas_pixels {
            return Err(UiError::AtlasLimit);
        }
        let identity = u64::try_from(existing.unwrap_or(self.atlases.len()))
            .map_err(|_| UiError::AtlasLimit)?;
        let mut draws = Vec::with_capacity(layout.glyphs.len() * passes);
        if let UiTextEffect::Shadow { color, offset } = effect {
            for glyph in &layout.glyphs {
                let mut color = color;
                color[3] *= run.color[3] * glyph.color[3];
                draws.push(self.glyph(
                    glyph,
                    placement,
                    offset,
                    color,
                    identity,
                    layout.atlas.size.as_vec2(),
                )?);
            }
        }
        for glyph in &layout.glyphs {
            draws.push(self.glyph(
                glyph,
                placement,
                Vec2::ZERO,
                multiply(run.color, glyph.color),
                identity,
                layout.atlas.size.as_vec2(),
            )?);
        }
        self.commit(draws)?;
        if references_atlas && existing.is_none() {
            self.presentation.coverage.insert(
                identity,
                CoverageTexture {
                    width: layout.atlas.size.x,
                    height: layout.atlas.size.y,
                    bytes: layout.atlas.alpha.clone(),
                },
            );
            self.atlases.push(layout.atlas.alpha.clone());
        }
        self.characters = characters;
        self.atlas_pixels = pixels;
        Ok(())
    }
    fn glyph(
        &self,
        glyph: &GlyphLayout,
        placement: UiTextPlacement,
        offset: Vec2,
        color: [f32; 4],
        identity: u64,
        atlas_size: Vec2,
    ) -> Result<super::UiDraw, UiError> {
        let (min, max) = (glyph.rect_min, glyph.rect_max);
        let scale = placement.destination.size / placement.layout_size;
        let points = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)].map(|p| {
            placement.destination.origin + Vec2::new(p.x, placement.layout_size.y - p.y) * scale
        });
        let points = rotate(
            points,
            placement.destination,
            placement.rotation_radians,
            placement.pivot,
        )
        .map(|p| p + offset);
        let low = glyph.atlas_min.as_vec2() / atlas_size;
        let high = (glyph.atlas_min + glyph.atlas_size).as_vec2() / atlas_size;
        self.quad(
            points,
            [low.x, low.y, high.x, high.y],
            color,
            Some(identity),
            UiDrawKind::Text,
        )
    }
}
fn validate_effect(effect: UiTextEffect) -> Result<(), UiError> {
    if let UiTextEffect::Shadow { color, offset } = effect {
        check_color(color)?;
        if !offset.is_finite() {
            return Err(UiError::InvalidRect);
        }
    }
    Ok(())
}
