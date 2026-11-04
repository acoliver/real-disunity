use crate::ui::{RectLayout, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World};
use glam::Vec2;
use std::collections::BTreeMap;

/// Lower-left pixel clip bounds, intersected with the canvas viewport and rounded inward.
pub(super) fn screen_clip(
    world: &World,
    layout: &UiLayout,
    rect: &RectLayout,
    roots: &BTreeMap<ObjectHandle, ObjectHandle>,
) -> [u32; 4] {
    let viewport = layout.viewport.as_vec2();
    let mut low = Vec2::ZERO;
    let mut high = viewport;
    let clip = world
        .published_components()
        .find_map(|component| match &component.desc {
            ComponentDesc::CanvasRenderer(renderer) if component.object == rect.object => {
                renderer.clip_rect
            }
            _ => None,
        });
    if let Some([x, y, width, height]) = clip.filter(|c| c.iter().all(|v| v.is_finite())) {
        let root = roots[&rect.canvas];
        let canvas = layout.rect(root).expect("resolved canvas rectangle");
        let min = canvas.drawn_point(glam::Vec3::new(x, y, 0.)).truncate();
        let max = canvas
            .drawn_point(glam::Vec3::new(x + width, y + height, 0.))
            .truncate();
        low = low.max(min).min(viewport).ceil();
        high = high.min(max).max(low).floor();
    }
    if let Some([min, max]) = rect.clip_bounds {
        low = low.max(min).min(viewport).ceil();
        high = high.min(max).floor().max(low);
    }
    let low = low.as_uvec2();
    let high = high.as_uvec2();
    [low.x, low.y, high.x - low.x, high.y - low.y]
}
