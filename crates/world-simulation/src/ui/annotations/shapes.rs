use super::{AnnotationPrimitive, MAX_ANNOTATION_SEGMENTS, draw, labels::Budget, point};
use crate::ui::{UiError, presentation::UiPresentation};
use glam::Vec2;
use worldrunner_camera::ResolvedView;

pub(super) fn extract(
    plan: &mut UiPresentation,
    budget: &mut Budget,
    primitive: &AnnotationPrimitive,
    view: &ResolvedView,
) -> Result<(), UiError> {
    match *primitive {
        AnnotationPrimitive::Line {
            from,
            to,
            width,
            colour,
        } => {
            positive(width)?;
            colour_valid(colour)?;
            if let (Some(a), Some(b)) = (point(from, view)?, point(to, view)?) {
                budget.vertices(6)?;
                line(plan, a, b, width, colour)?;
            }
        }
        AnnotationPrimitive::Arrow {
            from,
            to,
            width,
            head_length,
            head_width,
            colour,
        } => {
            arrow(
                plan,
                budget,
                view,
                (from, to),
                [width, head_length, head_width],
                colour,
            )?;
        }
        AnnotationPrimitive::Circle {
            centre,
            radius,
            thickness,
            colour,
            segments,
        } => {
            positive(radius)?;
            colour_valid(colour)?;
            if let Some(thickness) = thickness {
                positive(thickness)?;
            }
            if !(3..=MAX_ANNOTATION_SEGMENTS).contains(&segments) {
                return Err(UiError::AnnotationLimit);
            }
            if let Some(centre) = point(centre, view)? {
                budget.vertices(usize::from(segments) * if thickness.is_some() { 6 } else { 3 })?;
                circle(plan, centre, radius, thickness, colour, segments);
            }
        }
        AnnotationPrimitive::Label { .. } => unreachable!("labels use font extraction"),
    }
    Ok(())
}
fn arrow(
    plan: &mut UiPresentation,
    budget: &mut Budget,
    view: &ResolvedView,
    endpoints: (super::AnnotationPoint, super::AnnotationPoint),
    dimensions: [f32; 3],
    colour: [f32; 4],
) -> Result<(), UiError> {
    let [width, head_length, head_width] = dimensions;
    for value in dimensions {
        positive(value)?;
    }
    colour_valid(colour)?;
    if let (Some(a), Some(b)) = (point(endpoints.0, view)?, point(endpoints.1, view)?) {
        let direction = (b - a).try_normalize().ok_or(UiError::InvalidAnnotation)?;
        let base = if head_length >= a.distance(b) {
            a
        } else {
            b - direction * head_length
        };
        let side = Vec2::new(-direction.y, direction.x) * head_width * 0.5;
        budget.vertices(if base == a { 3 } else { 9 })?;
        if base != a {
            line(plan, a, base, width, colour)?;
        }
        triangle(plan, [base + side, base - side, b], colour);
    }
    Ok(())
}

fn positive(value: f32) -> Result<(), UiError> {
    if !value.is_finite() || value <= 0. {
        return Err(UiError::InvalidAnnotation);
    }
    Ok(())
}
fn colour_valid(colour: [f32; 4]) -> Result<(), UiError> {
    if colour.iter().any(|c| !(0. ..=1.).contains(c)) {
        return Err(UiError::InvalidAnnotation);
    }
    Ok(())
}
fn triangle(plan: &mut UiPresentation, points: [Vec2; 3], colour: [f32; 4]) {
    let [a, b, c] = points.map(|p| p.extend(0.).to_array());
    plan.draws.push(draw([a, b, c, c], colour));
}
fn line(
    plan: &mut UiPresentation,
    a: Vec2,
    b: Vec2,
    width: f32,
    colour: [f32; 4],
) -> Result<(), UiError> {
    let direction = (b - a).try_normalize().ok_or(UiError::InvalidAnnotation)?;
    let side = Vec2::new(-direction.y, direction.x) * width * 0.5;
    triangle(plan, [a - side, b - side, b + side], colour);
    triangle(plan, [a - side, b + side, a + side], colour);
    Ok(())
}
fn circle(
    plan: &mut UiPresentation,
    centre: Vec2,
    radius: f32,
    thickness: Option<f32>,
    colour: [f32; 4],
    segments: u16,
) {
    let inner = (radius - thickness.unwrap_or(radius)).max(0.);
    for index in 0..segments {
        let radial =
            |i| Vec2::from_angle(f32::from(i) * std::f32::consts::TAU / f32::from(segments));
        let (a, b) = (radial(index), radial(index + 1));
        if thickness.is_none() {
            triangle(
                plan,
                [centre, centre + a * radius, centre + b * radius],
                colour,
            );
        } else {
            triangle(
                plan,
                [centre + a * inner, centre + a * radius, centre + b * radius],
                colour,
            );
            triangle(
                plan,
                [centre + a * inner, centre + b * radius, centre + b * inner],
                colour,
            );
        }
    }
}
