use super::UiGlyphQuad;
use crate::ui::{GlyphLayout, RectLayout};
use glam::Vec2;

pub(super) fn quad(
    rect: &RectLayout,
    glyph: &GlyphLayout,
    atlas: Vec2,
    shift: Vec2,
) -> Option<UiGlyphQuad> {
    let glyph_min = glyph.rect_min + shift;
    let glyph_max = glyph.rect_max + shift;
    let low = glyph_min.max(Vec2::ZERO);
    let high = glyph_max.min(rect.size.max(Vec2::ZERO));
    if high.cmple(low).any() {
        return None;
    }
    let extent = glyph.rect_max - glyph.rect_min;
    let fraction_low = (low - glyph_min) / extent;
    let fraction_high = (high - glyph_min) / extent;
    let atlas_low = glyph.atlas_min.as_vec2();
    let atlas_extent = glyph.atlas_size.as_vec2();
    let uv_min =
        (atlas_low + atlas_extent * Vec2::new(fraction_low.x, 1. - fraction_high.y)) / atlas;
    let uv_max =
        (atlas_low + atlas_extent * Vec2::new(fraction_high.x, 1. - fraction_low.y)) / atlas;
    let min = low - rect.pivot * rect.size;
    let max = high - rect.pivot * rect.size;
    let positions = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    Some(UiGlyphQuad {
        link: glyph.link,
        corners: positions.map(|p| {
            rect.local_to_world
                .transform_point3(p.extend(0.))
                .to_array()
        }),
        uv: [
            [uv_min.x, uv_max.y],
            [uv_max.x, uv_max.y],
            [uv_max.x, uv_min.y],
            [uv_min.x, uv_min.y],
        ],
        color: glyph.color,
    })
}
