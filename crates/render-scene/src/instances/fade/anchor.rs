//! The point a placement's fade distance is measured from, which does not depend on the eye,
//! and a cheap estimate of that distance which settles most fades without interval arithmetic.
use glam::Vec3;
use worldrunner_render_plan::{SceneFadeReference, SceneTransform};

use super::span::Span;
use crate::geometry::{millimeter_to_meter, model_matrix};

/// Relative and absolute parts of the estimate's margin. The intervals widen by a few dozen
/// units in the last place of an f32 over the arithmetic; 2^-12 is thousands of them.
const MARGIN_RELATIVE: f64 = 1.0 / 4096.0;
const MARGIN_ABSOLUTE: f64 = 1.0 / 1_048_576.0;
/// Estimates this far out are left to the intervals. Below it every squared component and
/// their sum stay finite in f32, so the intervals are finite wherever the estimate decides.
const ESTIMATE_LIMIT: f64 = 1.0e18;

/// Outward intervals around a placement's fade reference point in world metres, and around
/// the lowest eye height its eye clamp admits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Anchor {
    center: [Span; 3],
    floor: Option<Span>,
}

impl Default for Anchor {
    fn default() -> Self {
        Self::origin([0.0; 3])
    }
}

impl Anchor {
    /// The placement origin in metres, which equals its model matrix's translation.
    pub fn origin(meters: [f32; 3]) -> Self {
        Self {
            center: meters.map(Span::point),
            floor: None,
        }
    }

    /// The authored reference transformed by the placement's model matrix.
    pub fn referenced(transform: &SceneTransform, authored: SceneFadeReference) -> Self {
        let model = model_matrix(transform);
        let mut center = model.w_axis.truncate().to_array().map(Span::point);
        let local = authored
            .offset_millimeters()
            .map(|v| Span::point(millimeter_to_meter(i64::from(v))));
        let linear = [model.x_axis, model.y_axis, model.z_axis].map(|v| v.to_array());
        for axis in 0..3 {
            center[axis] = Span::sum4([
                center[axis],
                Span::point(linear[0][axis]).mul(local[0]),
                Span::point(linear[1][axis]).mul(local[1]),
                Span::point(linear[2][axis]).mul(local[2]),
            ]);
        }
        Self {
            center,
            floor: authored
                .eye_vertical_clamp_millimeters()
                .map(|clamp| center[2].add(Span::point(millimeter_to_meter(i64::from(clamp))))),
        }
    }

    /// The middle of the reference point's intervals.
    #[cfg(test)]
    pub fn middle(self) -> Vec3 {
        Vec3::from_array(self.center.map(|span| span.lo * 0.5 + span.hi * 0.5))
    }

    /// Outward bounds of the distance the stock shader measures from `eye`.
    pub(super) fn span(self, eye: Vec3) -> Span {
        let mut eye = eye.to_array().map(Span::point);
        if let Some(floor) = self.floor {
            eye[2] = eye[2].maximum(floor);
        }
        Span::length(std::array::from_fn(|axis| eye[axis].sub(self.center[axis])))
    }

    /// The distance from `eye` in f64, from the middle of each interval, with a margin that
    /// holds both bounds of `span(eye)`; none when the inputs are not finite or very large.
    pub(super) fn estimate(self, eye: Vec3) -> Option<Estimate> {
        let middle = |span: Span| (f64::from(span.lo) + f64::from(span.hi)) * 0.5;
        let half = |span: Span| (f64::from(span.hi) - f64::from(span.lo)) * 0.5;
        let size = |span: Span| f64::from(span.lo.abs().max(span.hi.abs()));
        let mut magnitude = 0.0_f64;
        let mut widths = 0.0;
        let mut squared = 0.0;
        for (axis, eye) in eye.to_array().into_iter().enumerate() {
            let center = self.center[axis];
            let mut eye = f64::from(eye);
            magnitude = magnitude.max(eye.abs()).max(size(center));
            widths += half(center);
            if let (2, Some(floor)) = (axis, self.floor) {
                magnitude = magnitude.max(size(floor));
                widths += half(floor);
                eye = eye.max(middle(floor));
            }
            let delta = eye - middle(center);
            squared += delta * delta;
        }
        let distance = squared.sqrt();
        let margin = 4.0 * widths + (magnitude + distance) * MARGIN_RELATIVE + MARGIN_ABSOLUTE;
        (distance + margin < ESTIMATE_LIMIT).then_some(Estimate { distance, margin })
    }
}

/// A distance known to lie within `margin` of `distance`, as do both bounds of its interval.
#[derive(Clone, Copy, Debug)]
pub(super) struct Estimate {
    distance: f64,
    margin: f64,
}

impl Estimate {
    #[cfg(test)]
    pub fn bounds(self) -> [f64; 2] {
        [self.distance - self.margin, self.distance + self.margin]
    }

    /// The opacity `distance_coverage` returns for the interval, when the interval's place
    /// against the directed `band` settles it: entirely before or after the band, or, for a
    /// terminal fade, far enough past its end that the falling factor clamps to zero.
    pub fn coverage(self, [start, end]: [f32; 2]) -> Option<Span> {
        let low = self.distance - self.margin;
        let high = self.distance + self.margin;
        let (start_meters, end_meters) = (f64::from(start), f64::from(end));
        if start < 0.0 {
            // A step band: the marker says which side of `end` keeps full opacity.
            let before = start.to_bits() == (-2.0_f32).to_bits();
            return if high <= end_meters {
                Some(Span::point(f32::from(before)))
            } else if low > end_meters {
                Some(Span::point(f32::from(!before)))
            } else {
                None
            };
        }
        if end < 0.0 {
            if high <= start_meters {
                return Some(Span::point(1.0));
            }
            // At least one margin past the end, `-end - distance` stays a negative normal
            // through the division and its rounding, and the clamp makes both bounds zero.
            return (low > start_meters && low >= -end_meters + self.margin)
                .then_some(Span::point(0.0));
        }
        if high < start_meters {
            return Some(Span::point(0.0));
        }
        (low >= start_meters && low > end_meters).then_some(Span::point(1.0))
    }
}
