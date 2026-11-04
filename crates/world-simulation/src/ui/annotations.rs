//! Physical-pixel annotations projected with the caller's immutable presented view.
mod labels;
mod shapes;
use super::{
    FontBook, TextRun, UiError,
    presentation::{UiDraw, UiDrawKind, UiPresentation},
};
use glam::{Vec2, Vec3};
use worldrunner_camera::ResolvedView;

pub const MAX_ANNOTATION_SEGMENTS: u16 = 256;
/// Expanded triangle-list vertices, including label quads, across one extraction.
pub const MAX_ANNOTATION_VERTICES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnnotationPoint {
    /// Lower-left physical viewport pixels.
    Screen(Vec2),
    /// Metres in the presented view's world basis (scene hosts use Z-up).
    World(Vec3),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationDepth {
    AlwaysVisible,
    /// Unsupported by the display overlay, which has no scene-depth attachment.
    DepthTested,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationPrimitive {
    Line {
        from: AnnotationPoint,
        to: AnnotationPoint,
        width: f32,
        colour: [f32; 4],
    },
    Arrow {
        from: AnnotationPoint,
        to: AnnotationPoint,
        width: f32,
        head_length: f32,
        head_width: f32,
        colour: [f32; 4],
    },
    Circle {
        centre: AnnotationPoint,
        radius: f32,
        thickness: Option<f32>,
        colour: [f32; 4],
        segments: u16,
    },
    /// Alignment is the normalized pivot of the rasterized text bounds.
    Label {
        anchor: AnnotationPoint,
        run: TextRun,
        alignment: Vec2,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub primitive: AnnotationPrimitive,
    /// Ascending layer, order, hierarchy, matching ordinary UI contributions.
    pub draw_order: (i32, i32, usize),
    pub clip_rect: Option<[u32; 4]>,
    pub depth: AnnotationDepth,
}

/// Extracts atomically. Invalid settings and exceeded budgets are errors, never partial output.
/// Anchors outside the six camera clip planes suppress their primitive. Screen geometry is
/// clipped by viewport/scissor; world endpoints must both be visible (no edge wrapping).
pub fn extract_annotations(
    annotations: &[Annotation],
    fonts: &FontBook,
    view: &ResolvedView,
) -> Result<UiPresentation, UiError> {
    let viewport = view.viewport();
    if viewport.width.max(viewport.height) > 16384 {
        return Err(UiError::InvalidViewport);
    }
    let mut result = UiPresentation::default();
    let mut budget = labels::Budget::default();
    for annotation in annotations {
        if annotation.depth != AnnotationDepth::AlwaysVisible {
            return Err(UiError::UnsupportedAnnotationDepth);
        }
        let start = result.draws.len();
        match &annotation.primitive {
            AnnotationPrimitive::Label {
                anchor,
                run,
                alignment,
            } => {
                labels::extract(
                    &mut result,
                    &mut budget,
                    fonts,
                    view,
                    (*anchor, run, *alignment),
                )?;
            }
            primitive => shapes::extract(&mut result, &mut budget, primitive, view)?,
        }
        for draw in &mut result.draws[start..] {
            if draw.corners.iter().flatten().any(|v| !v.is_finite()) {
                return Err(UiError::InvalidAnnotation);
            }
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order) = annotation.draw_order;
            draw.clip_rect =
                annotation
                    .clip_rect
                    .unwrap_or([0, 0, viewport.width, viewport.height]);
        }
    }
    Ok(result)
}

fn point(point: AnnotationPoint, view: &ResolvedView) -> Result<Option<Vec2>, UiError> {
    match point {
        AnnotationPoint::Screen(p) if p.is_finite() => Ok(Some(p)),
        AnnotationPoint::World(p) if p.is_finite() => {
            if !view.contains_point(p) {
                return Ok(None);
            }
            let projected = view
                .world_to_screen(p)
                .map_err(|_| UiError::InvalidAnnotation)?;
            Ok(Some(Vec2::new(
                projected.x,
                f32::from(
                    u16::try_from(view.viewport().height).map_err(|_| UiError::InvalidViewport)?,
                ) - projected.y,
            )))
        }
        _ => Err(UiError::InvalidAnnotation),
    }
}

fn draw(corners: [[f32; 3]; 4], colour: [f32; 4]) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        corners,
        uv_rect: [0.; 4],
        color: colour,
        texture: None,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0; 4],
        triangle_uv: Some([[0.; 2]; 3]),
    }
}
