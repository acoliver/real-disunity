use super::UiGraphicContribution;
use crate::ui::{RectLayout, TextLayout, UiGraphic, UiGraphicQuad, text_geometry};
use glam::Vec2;

pub(super) fn contributions(
    rect: &RectLayout,
    text: &TextLayout,
    shift: Vec2,
    font_size: f32,
    color: [f32; 4],
    clip_rect: [u32; 4],
) -> Vec<UiGraphicContribution> {
    let thickness = (font_size / 16.).max(1.);
    let mut result: Vec<UiGraphicContribution> = Vec::new();
    for segment in text_geometry::segments(text, shift) {
        let bounds = [
            Vec2::new(segment.low.x, segment.baseline - 2. * thickness),
            Vec2::new(segment.high.x, segment.baseline - thickness),
        ];
        let Some([low, high]) = text_geometry::clipped(rect, bounds) else {
            continue;
        };
        let positions = [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ];
        let quad = UiGraphicQuad {
            corners: positions.map(|p| {
                rect.drawn_point((p - rect.pivot * rect.size).extend(0.))
                    .to_array()
            }),
            uv_rect: [0., 0., 1., 1.],
            color: std::array::from_fn(|i| color[i] * segment.color[i]),
        };
        if let Some(entry) = result.iter_mut().find(|c| c.link == Some(segment.link)) {
            entry.graphic.quads.push(quad);
        } else {
            result.push(UiGraphicContribution {
                graphic: UiGraphic {
                    object: rect.object,
                    canvas: rect.canvas,
                    overlay: rect.on_screen(),
                    sorting_layer: rect.sorting_layer,
                    sorting_order: rect.sorting_order,
                    texture: None,
                    quads: vec![quad],
                    left_out_triangles: 0,
                },
                link: Some(segment.link),
                hierarchy_order: rect.hierarchy_order,
                clip_rect,
            });
        }
    }
    result
}
