use super::UiTextField;
use crate::ui::{
    FontBook, RectLayout, UiError, UiGraphic, UiGraphicQuad, UiRouter,
    contribution::{self, UiGraphicContribution, UiRenderPlan},
};
use crate::{World, WorldError};
use glam::{Vec2, Vec3};

/// Caller-authored colours, without a separate draw path or implicit theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFieldStyle {
    pub caret: [f32; 4],
    pub selection: [f32; 4],
}
impl UiRouter {
    pub fn text_fields_plan(
        &self,
        world: &World,
        fonts: &FontBook,
        plan: &mut UiRenderPlan,
        style: TextFieldStyle,
    ) -> Result<(), WorldError> {
        if style
            .caret
            .into_iter()
            .chain(style.selection)
            .any(|c| !(0. ..=1.).contains(&c))
        {
            return Err(UiError::InvalidText.into());
        }
        for (object, field) in &self.text_fields {
            if !self
                .controls
                .get(object)
                .is_some_and(|c| c.kind == crate::ui::UiControlKind::TextField)
            {
                continue;
            }
            if plan.layout.rect(*object).is_none() {
                continue;
            }
            let focused = self.text_target(world, &plan.layout)? == Some(*object);
            field.append(world, fonts, plan, (*object, focused), style)?;
        }
        plan.texts
            .sort_by_key(|t| (t.sorting_layer, t.sorting_order, t.hierarchy_order));
        plan.graphics.sort_by_key(|g| {
            (
                g.graphic.sorting_layer,
                g.graphic.sorting_order,
                g.hierarchy_order,
            )
        });
        Ok(())
    }
}
impl UiTextField {
    fn geometry(
        &self,
        fonts: &FontBook,
        rect: &RectLayout,
    ) -> Result<(super::TextRun, Vec<f32>, f32), UiError> {
        let (run, cursor) = self.displayed();
        let positions = fonts.text_positions(&run)?;
        let offset = (positions[cursor] + 1. - rect.size.x).max(0.);
        Ok((run, positions, offset))
    }
    pub(in crate::ui) fn caret_area(
        &self,
        fonts: &FontBook,
        rect: &RectLayout,
        height: f32,
    ) -> Result<[f64; 4], UiError> {
        let (_, positions, offset) = self.geometry(fonts, rect)?;
        let cursor = self.displayed().1;
        let local = Vec3::new(
            positions[cursor] - offset,
            (rect.size.y - self.run.font_size) * 0.5,
            0.,
        );
        let a = rect.drawn_point(local - (rect.pivot * rect.size).extend(0.));
        let b = rect.drawn_point(
            local + Vec3::new(1., self.run.font_size, 0.) - (rect.pivot * rect.size).extend(0.),
        );
        let min = a.min(b);
        let max = a.max(b);
        Ok([
            f64::from(min.x),
            f64::from(height - max.y),
            f64::from(max.x - min.x),
            f64::from(max.y - min.y),
        ])
    }
    fn append(
        &self,
        world: &World,
        fonts: &FontBook,
        plan: &mut UiRenderPlan,
        target: (crate::ObjectHandle, bool),
        style: TextFieldStyle,
    ) -> Result<(), WorldError> {
        let rect = plan.layout.rect(target.0).expect("resolved field");
        if rect.mode != crate::ui::CanvasMode::ScreenSpaceOverlay || rect.size.min_element() <= 0. {
            return Err(UiError::InvalidRect.into());
        }
        let (run, positions, offset) = self.geometry(fonts, rect)?;
        let canvas = world
            .published_components()
            .find_map(|c| match c.desc {
                crate::ComponentDesc::Canvas(canvas) if c.object == rect.canvas => Some(canvas),
                _ => None,
            })
            .unwrap_or_default();
        let layout = fonts.layout(&run, rect.size)?;
        let mut text = contribution::contribution(rect, &layout, true, run.color, canvas);
        let clip = field_clip(world, &plan.layout, rect)?;
        text.clip_rect = clip;
        let shift = rect
            .local_to_world
            .transform_vector3(Vec3::new(offset, 0., 0.));
        for quad in &mut text.quads {
            for corner in &mut quad.corners {
                *corner = (Vec3::from(*corner) - shift).to_array();
            }
        }
        let mut quads = Vec::new();
        if target.1 {
            let range = self.selection();
            if !range.is_empty() && self.preedit.is_empty() {
                quads.push(quad(
                    rect,
                    [
                        positions[range.start] - offset,
                        positions[range.end] - offset,
                    ],
                    style.selection,
                ));
            }
            let cursor = self.displayed().1;
            quads.push(quad(
                rect,
                [positions[cursor] - offset, positions[cursor] - offset + 1.],
                style.caret,
            ));
        }
        plan.graphics.push(UiGraphicContribution {
            hierarchy_order: rect.hierarchy_order,
            clip_rect: clip,
            graphic: UiGraphic {
                object: rect.object,
                canvas: rect.canvas,
                overlay: true,
                sorting_layer: rect.sorting_layer,
                sorting_order: rect.sorting_order,
                texture: None,
                quads,
                left_out_triangles: 0,
            },
        });
        plan.texts.push(text);
        Ok(())
    }
}
fn field_clip(
    world: &World,
    layout: &crate::ui::UiLayout,
    rect: &RectLayout,
) -> Result<[u32; 4], WorldError> {
    let clip = contribution::field_clip(world, layout, rect)?;
    let corners = rect.screen_corners.ok_or(UiError::InvalidRect)?;
    let min = corners
        .into_iter()
        .fold(Vec2::splat(f32::INFINITY), Vec2::min)
        .max(glam::UVec2::new(clip[0], clip[1]).as_vec2())
        .ceil();
    let max = corners
        .into_iter()
        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max)
        .min(glam::UVec2::new(clip[0] + clip[2], clip[1] + clip[3]).as_vec2())
        .floor()
        .max(min);
    let min = min.as_uvec2();
    let max = max.as_uvec2();
    Ok([min.x, min.y, max.x - min.x, max.y - min.y])
}
fn quad(rect: &RectLayout, x: [f32; 2], color: [f32; 4]) -> UiGraphicQuad {
    let min = Vec2::new(x[0], 0.) - rect.pivot * rect.size;
    let max = Vec2::new(x[1], rect.size.y) - rect.pivot * rect.size;
    UiGraphicQuad {
        corners: [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
            .map(|p| rect.drawn_point(p.extend(0.)).to_array()),
        uv_rect: [0., 0., 1., 1.],
        color,
    }
}
