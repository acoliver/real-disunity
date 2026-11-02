//! Rect clipping of a canvas renderer's quads. The clip rectangle is given in the space of the
//! root canvas (x, y, width, height); a quad is clipped there, before its corners become pixels.
use glam::{Mat4, Vec2, Vec3};

/// How far from the extreme of its bounding box a corner may be and still count as on that
/// edge, relative to the box size (at least 1).
const EDGE_TOLERANCE: f32 = 1e-4;

/// The clip rectangle of one renderer and the root canvas it is measured in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CanvasClip {
    min: Vec2,
    max: Vec2,
    canvas_from_world: Mat4,
    world_from_canvas: Mat4,
}

/// What the clip rectangle does to one quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Clipped {
    /// The quad is drawn as it is.
    Whole,
    /// No part of the quad is inside the rectangle.
    Outside,
    /// The part inside the rectangle: world corners and texture rectangle.
    Cut([Vec3; 4], [f32; 4]),
}

/// The bounding box of four canvas-space corners that form an axis aligned rectangle, and where
/// each corner sits in it: 0 or 1 on each axis.
struct Frame {
    min: Vec2,
    max: Vec2,
    sides: [Vec2; 4],
}

impl CanvasClip {
    /// `rect` is x, y, width, height in the canvas space of `canvas_to_world`. A rectangle that
    /// is not finite, or a canvas that cannot be inverted, clips nothing (the engine's choice).
    pub(crate) fn new(rect: [f32; 4], canvas_to_world: Mat4) -> Option<Self> {
        let inverse = canvas_to_world.inverse();
        if !rect.iter().all(|v| v.is_finite()) || !inverse.is_finite() {
            return None;
        }
        Some(Self {
            min: Vec2::new(rect[0], rect[1]),
            max: Vec2::new(rect[0] + rect[2], rect[1] + rect[3]),
            canvas_from_world: inverse,
            world_from_canvas: canvas_to_world,
        })
    }

    /// Clips the quad with the world corners `world` and texture rectangle `uv_rect` (as
    /// `UiGraphicQuad::uv_rect`). A quad that is not axis aligned in canvas space is kept whole.
    pub(crate) fn clip(&self, world: [Vec3; 4], uv_rect: [f32; 4]) -> Clipped {
        let canvas = world.map(|p| self.canvas_from_world.transform_point3(p));
        let Some(frame) = Frame::of(&canvas.map(Vec3::truncate)) else {
            return Clipped::Whole;
        };
        let low = frame.min.max(self.min);
        let high = frame.max.min(self.max);
        if low.x >= high.x || low.y >= high.y {
            return Clipped::Outside;
        }
        if low.cmple(frame.min).all() && high.cmpge(frame.max).all() {
            return Clipped::Whole;
        }
        let kept: [Vec2; 4] = std::array::from_fn(|i| low + (high - low) * frame.sides[i]);
        let corners = std::array::from_fn(|i| {
            self.world_from_canvas
                .transform_point3(kept[i].extend(canvas[i].z))
        });
        Clipped::Cut(corners, frame.texture_rect(&kept, uv_rect))
    }
}

impl Frame {
    fn of(points: &[Vec2; 4]) -> Option<Self> {
        let min = points.iter().fold(Vec2::MAX, |low, p| low.min(*p));
        let max = points.iter().fold(Vec2::MIN, |high, p| high.max(*p));
        let tolerance = EDGE_TOLERANCE * (max - min).max_element().max(1.);
        let mut sides = [Vec2::ZERO; 4];
        for (side, point) in sides.iter_mut().zip(points) {
            *side = Vec2::new(
                edge(point.x - min.x, max.x - point.x, tolerance)?,
                edge(point.y - min.y, max.y - point.y, tolerance)?,
            );
        }
        let distinct =
            (0..4).all(|i| (i + 1..4).all(|j| sides[i].distance_squared(sides[j]) > 0.5));
        distinct.then_some(Self { min, max, sides })
    }

    /// The texture rectangle of the part of the quad whose corners are now `kept`. Texture
    /// coordinates change linearly across the quad, so each kept corner takes the blend of the
    /// original corners' coordinates at its place in the original box.
    fn texture_rect(&self, kept: &[Vec2; 4], uv_rect: [f32; 4]) -> [f32; 4] {
        let [u0, row_top, u1, row_bottom] = uv_rect;
        let columns = [u0, u1, u1, u0];
        let rows = [row_bottom, row_bottom, row_top, row_top];
        let at = |corner: usize| (kept[corner] - self.min) / (self.max - self.min);
        [
            self.blend(columns, at(0)),
            self.blend(rows, at(2)),
            self.blend(columns, at(2)),
            self.blend(rows, at(0)),
        ]
    }

    /// Bilinear blend of one value per original corner at `place`, a fraction of the box.
    fn blend(&self, values: [f32; 4], place: Vec2) -> f32 {
        self.sides
            .iter()
            .zip(values)
            .map(|(side, value)| {
                let weight = Vec2::ONE - (place - *side).abs();
                weight.x * weight.y * value
            })
            .sum()
    }
}

/// 0 when `from_min` is within `tolerance`, 1 when `from_max` is, otherwise not an edge.
fn edge(from_min: f32, from_max: f32, tolerance: f32) -> Option<f32> {
    if from_min <= tolerance {
        Some(0.)
    } else if from_max <= tolerance {
        Some(1.)
    } else {
        None
    }
}
