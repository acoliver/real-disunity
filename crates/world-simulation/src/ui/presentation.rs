//! Detached screen-space draws from the world's UI extraction, without live object handles.
use super::contribution::UiRenderPlan;
use crate::presentation::CoverageTexture;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiDrawKind {
    Graphic,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiDraw {
    pub kind: UiDrawKind,
    /// Lower-left viewport pixels, in bottom-left, bottom-right, top-right, top-left order.
    pub corners: [[f32; 3]; 4],
    pub uv_rect: [f32; 4],
    pub color: [f32; 4],
    /// Graphic image identity, or an identity in `UiPresentation::coverage` for text.
    pub texture: Option<u64>,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub hierarchy_order: usize,
    pub clip_rect: [u32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPresentation {
    pub draws: Vec<UiDraw>,
    pub coverage: BTreeMap<u64, CoverageTexture>,
}

impl UiPresentation {
    /// Preserves pixel coordinates, clipping and `RectLayout::draw_order()` before the live
    /// 2D path converts UI to world-space sprite quads. Non-overlay canvases stay on that path.
    pub fn from_plan(plan: &UiRenderPlan) -> Self {
        let mut result = Self::default();
        for contribution in &plan.graphics {
            let graphic = &contribution.graphic;
            if !graphic.overlay {
                continue;
            }
            for quad in &graphic.quads {
                result.draws.push(UiDraw {
                    kind: UiDrawKind::Graphic,
                    corners: quad.corners,
                    uv_rect: quad.uv_rect,
                    color: quad.color,
                    texture: graphic.texture,
                    sorting_layer: graphic.sorting_layer,
                    sorting_order: graphic.sorting_order,
                    hierarchy_order: contribution.hierarchy_order,
                    clip_rect: contribution.clip_rect,
                });
            }
        }
        result.append_text(plan);
        result.draws.sort_by_key(|draw| {
            (
                draw.sorting_layer,
                draw.sorting_order,
                draw.hierarchy_order,
                draw.kind,
            )
        });
        result
    }

    fn append_text(&mut self, plan: &UiRenderPlan) {
        for (index, text) in plan.texts.iter().enumerate().filter(|(_, t)| t.overlay) {
            let identity = index as u64;
            if let Some(texture) = &text.texture {
                self.coverage.insert(identity, texture.clone());
            }
            for glyph in &text.quads {
                let uv = glyph.uv;
                self.draws.push(UiDraw {
                    kind: UiDrawKind::Text,
                    corners: glyph.corners,
                    uv_rect: [uv[0][0], uv[2][1], uv[2][0], uv[0][1]],
                    color: text.color,
                    texture: text.texture.as_ref().map(|_| identity),
                    sorting_layer: text.sorting_layer,
                    sorting_order: text.sorting_order,
                    hierarchy_order: text.hierarchy_order,
                    clip_rect: text.clip_rect,
                });
            }
        }
    }
}
