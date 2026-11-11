//! Per-material fade selection data, using the stock vertex and fragment rules.
use glam::Vec3;
use worldrunner_render_plan::{
    SceneFadeBand, SceneFadeCoverage, SceneFadeDirection, SceneFadeQuantization,
    SceneFadeReference, SceneMaterial, SceneObject, SceneTexture, threshold_texels,
};

use crate::geometry::{fade_band_meters, millimeter_to_meter, model_matrix};
mod span;
use span::Span;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fade {
    band: Option<SceneFadeBand>,
    reference: Option<SceneFadeReference>,
    direction: SceneFadeDirection,
    quantization: SceneFadeQuantization,
    coverage: SceneFadeCoverage,
    /// Whether every texel of the material's threshold grid is above zero, so zero opacity
    /// keeps no fragment. False for hashed coverage and for a grid outside `textures`.
    threshold_above_zero: bool,
}

impl Fade {
    /// `textures` is the table the material's threshold handle indexes.
    pub fn new(material: &SceneMaterial, textures: &[SceneTexture]) -> Self {
        let threshold_above_zero = match material.fade_coverage() {
            SceneFadeCoverage::Hashed => false,
            SceneFadeCoverage::Threshold { texture, .. } => textures
                .get(texture.raw() as usize)
                .and_then(threshold_texels)
                .is_some_and(|(_, texels)| !texels.is_empty() && !texels.contains(&0)),
        };
        Self {
            band: material
                .impostor()
                .and_then(worldrunner_render_plan::SceneImpostor::fade),
            reference: material.fade_reference(),
            direction: material.fade_direction(),
            quantization: material.fade_quantization(),
            coverage: material.fade_coverage(),
            threshold_above_zero,
        }
    }

    pub fn empty(self, object: SceneObject, eye: Vec3) -> bool {
        let Some(band) = object.fade().or(self.band) else {
            return false;
        };
        if !eye.is_finite() {
            return false;
        }
        self.empty_at(self.distance(object, eye), fade_band_meters(band))
    }

    fn empty_at(self, distance: Span, mut band: [f32; 2]) -> bool {
        if !distance.lo.is_finite() || !distance.hi.is_finite() {
            return false;
        }
        if self.direction == SceneFadeDirection::Complement {
            band = if band[0] < 0.0 {
                [
                    if band[0].to_bits() == (-2.0_f32).to_bits() {
                        -1.0
                    } else {
                        -2.0
                    },
                    band[1],
                ]
            } else {
                [band[0], -band[1]]
            };
        }
        let mut opacity = distance_coverage(distance, band);
        if self.quantization == SceneFadeQuantization::Byte {
            opacity = opacity.byte();
        }
        let hashed = matches!(self.coverage, SceneFadeCoverage::Hashed);
        match self.direction {
            // Inclusive coverage keeps opacity at or above the threshold, so zero opacity
            // survives a grid holding any zero texel.
            SceneFadeDirection::Inclusive => {
                (hashed || self.threshold_above_zero) && opacity.lo == 0.0 && opacity.hi == 0.0
            }
            SceneFadeDirection::Keep => opacity.lo == 0.0 && opacity.hi == 0.0,
            // Complement stores the shared lower boundary, not 1 minus opacity.
            // An R8 threshold of one still owns the equality at this boundary.
            SceneFadeDirection::Complement => {
                hashed
                    && opacity.lo.to_bits() == 1.0_f32.to_bits()
                    && opacity.hi.to_bits() == 1.0_f32.to_bits()
            }
        }
    }

    fn distance(self, object: SceneObject, eye: Vec3) -> Span {
        let model = model_matrix(object.transform());
        let mut reference = model.w_axis.truncate().to_array().map(Span::point);
        let mut eye = eye.to_array().map(Span::point);
        if let Some(authored) = self.reference {
            let local = authored
                .offset_millimeters()
                .map(|v| Span::point(millimeter_to_meter(i64::from(v))));
            let linear = [model.x_axis, model.y_axis, model.z_axis].map(|v| v.to_array());
            for axis in 0..3 {
                reference[axis] = Span::sum4([
                    reference[axis],
                    Span::point(linear[0][axis]).mul(local[0]),
                    Span::point(linear[1][axis]).mul(local[1]),
                    Span::point(linear[2][axis]).mul(local[2]),
                ]);
            }
            if let Some(clamp) = authored.eye_vertical_clamp_millimeters() {
                eye[2] = eye[2]
                    .maximum(reference[2].add(Span::point(millimeter_to_meter(i64::from(clamp)))));
            }
        }
        Span::length(std::array::from_fn(|axis| eye[axis].sub(reference[axis])))
    }
}

fn distance_coverage(distance: Span, [start, end]: [f32; 2]) -> Span {
    if start < 0.0 {
        if distance.lo <= end && distance.hi > end {
            return Span { lo: 0.0, hi: 1.0 };
        }
        return Span::point(f32::from(
            (distance.hi <= end) == (start.to_bits() == (-2.0_f32).to_bits()),
        ));
    }
    if end < 0.0 {
        if distance.hi <= start {
            return Span::point(1.0);
        }
        if distance.lo <= start {
            return Span { lo: 0.0, hi: 1.0 };
        }
        return Span::point(-end)
            .sub(distance)
            .div(Span::point(-end).sub(Span::point(start)).max(1.0))
            .max(0.0);
    }
    if distance.hi < start {
        return Span::point(0.0);
    }
    if distance.lo > end {
        return Span::point(1.0);
    }
    if distance.lo < start || distance.hi > end {
        return Span { lo: 0.0, hi: 1.0 };
    }
    Span::point(1.0).sub(
        Span::point(end)
            .sub(distance)
            .div(Span::point(end).sub(Span::point(start)).max(1.0)),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod distance_tests;
