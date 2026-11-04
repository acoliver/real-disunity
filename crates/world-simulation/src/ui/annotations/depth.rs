use super::{AnnotationPoint, AnnotationPrimitive, point};
use crate::ui::{UiError, presentation::UiDraw};
use glam::Vec2;
use worldrunner_camera::ResolvedView;

pub(super) struct Depth {
    start: Vec2,
    direction: Vec2,
    depths: [f32; 2],
}
impl Depth {
    pub fn new(
        primitive: &AnnotationPrimitive,
        view: &ResolvedView,
    ) -> Result<Option<Self>, UiError> {
        let (a, b) = match *primitive {
            AnnotationPrimitive::Line { from, to, .. }
            | AnnotationPrimitive::Arrow { from, to, .. } => (from, to),
            AnnotationPrimitive::Circle { centre, .. } => (centre, centre),
            AnnotationPrimitive::Label { anchor, .. } => (anchor, anchor),
        };
        let (AnnotationPoint::World(a), AnnotationPoint::World(b)) = (a, b) else {
            return Err(UiError::UnsupportedAnnotationDepth);
        };
        if !view.contains_point(a) || !view.contains_point(b) {
            return Ok(None);
        }
        let screen = |p| {
            view.world_to_screen(p)
                .map_err(|_| UiError::InvalidAnnotation)
        };
        let (pa, pb) = (screen(a)?, screen(b)?);
        let start = point(AnnotationPoint::World(a), view)?.expect("anchor inside frustum");
        let end = point(AnnotationPoint::World(b), view)?.expect("anchor inside frustum");
        Ok(Some(Self {
            start,
            direction: end - start,
            depths: [pa.z, pb.z],
        }))
    }
    pub fn apply(&self, draw: &mut UiDraw) {
        draw.depth_test = true;
        let length = self.direction.length_squared();
        for corner in &mut draw.corners {
            let t = if length == 0. {
                0.
            } else {
                ((Vec2::new(corner[0], corner[1]) - self.start).dot(self.direction) / length)
                    .clamp(0., 1.)
            };
            corner[2] = self.depths[0] + (self.depths[1] - self.depths[0]) * t;
        }
    }
}
