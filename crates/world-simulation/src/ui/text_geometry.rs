//! Shared local text geometry for extraction, link hits and focus reveal.
use super::{Canvas, RectLayout, TextLayout, UiLinkId};
use glam::{Vec2, Vec3};

pub(super) fn snap_offset(text: &TextLayout, rect: &RectLayout, canvas: Canvas) -> Vec2 {
    let Some(pixels) = rect.pixels_from_world.filter(|_| canvas.pixel_perfect) else {
        return Vec2::ZERO;
    };
    let Some(first) = text.glyphs.first() else {
        return Vec2::ZERO;
    };
    let matrix = pixels * rect.local_to_world;
    if matrix.determinant() == 0. {
        return Vec2::ZERO;
    }
    let corner = Vec2::new(first.rect_min.x, first.rect_max.y) - rect.pivot * rect.size;
    let screen = matrix.transform_point3(corner.extend(0.));
    let offset = (screen.round() - screen) * Vec3::new(1., 1., 0.);
    matrix.inverse().transform_vector3(offset).truncate()
}

pub(super) fn clipped(rect: &RectLayout, bounds: [Vec2; 2]) -> Option<[Vec2; 2]> {
    let low = bounds[0].max(Vec2::ZERO);
    let high = bounds[1].min(rect.size.max(Vec2::ZERO));
    high.cmpgt(low).all().then_some([low, high])
}

pub(super) struct LinkSegment {
    pub link: UiLinkId,
    pub low: Vec2,
    pub high: Vec2,
    pub baseline: f32,
    pub color: [f32; 4],
}

pub(super) fn segments(text: &TextLayout, shift: Vec2) -> Vec<LinkSegment> {
    let mut result: Vec<LinkSegment> = Vec::new();
    for glyph in &text.glyphs {
        let Some(link) = glyph.link else { continue };
        let baseline = glyph.origin.y + shift.y;
        let low = glyph.rect_min + shift;
        let high = glyph.rect_max + shift;
        if let Some(last) = result.last_mut()
            && last.link == link
            && last.baseline.to_bits() == baseline.to_bits()
        {
            last.low = last.low.min(low);
            last.high = last.high.max(high);
        } else {
            result.push(LinkSegment {
                link,
                low,
                high,
                baseline,
                color: glyph.color,
            });
        }
    }
    result
}

pub(super) fn canvas(world: &crate::World, object: crate::ObjectHandle) -> Canvas {
    world
        .published_components()
        .find_map(|c| match c.desc {
            crate::ComponentDesc::Canvas(canvas) if c.object == object => Some(canvas),
            _ => None,
        })
        .unwrap_or_default()
}
