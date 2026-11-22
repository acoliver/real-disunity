//! Per-material fade selection data, using the stock vertex and fragment rules.
use glam::Vec3;
use worldrunner_render_plan::{
    SceneFadeCoverage, SceneFadeDirection, SceneFadeQuantization, SceneFadeReference,
    SceneMaterial, SceneTexture, threshold_texels,
};

use crate::geometry::fade_band_meters;
mod anchor;
mod span;
pub(crate) use anchor::Anchor;
use anchor::Estimate;
use span::Span;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fade {
    /// The impostor's fade band in metres, as `fade_band_meters` converts it.
    band: Option<[f32; 2]>,
    reference: Option<SceneFadeReference>,
    direction: SceneFadeDirection,
    quantization: SceneFadeQuantization,
    coverage: SceneFadeCoverage,
    /// Whether every texel of the material's threshold grid is above zero, so zero opacity
    /// keeps no fragment. False for hashed coverage and for a grid outside `textures`.
    threshold_above_zero: bool,
}

impl Fade {
    /// `textures` is the validated table the material's threshold handle indexes.
    pub fn new(material: &SceneMaterial, textures: &[SceneTexture]) -> Self {
        let threshold_above_zero = match material.fade_coverage() {
            SceneFadeCoverage::Hashed => false,
            SceneFadeCoverage::Threshold { texture, .. } => {
                let (_, texels) = threshold_texels(&textures[texture.raw() as usize])
                    .expect("validated threshold grid");
                !texels.is_empty() && !texels.contains(&0)
            }
        };
        Self {
            band: material
                .impostor()
                .and_then(worldrunner_render_plan::SceneImpostor::fade)
                .map(fade_band_meters),
            reference: material.fade_reference(),
            direction: material.fade_direction(),
            quantization: material.fade_quantization(),
            coverage: material.fade_coverage(),
            threshold_above_zero,
        }
    }

    /// Whether some distance can fade out every fragment. Inclusive coverage keeps zero
    /// opacity on a grid holding a zero texel, and complement threshold coverage keeps the
    /// shared boundary, so those keep fragments at every distance.
    pub fn can_empty(self) -> bool {
        let hashed = matches!(self.coverage, SceneFadeCoverage::Hashed);
        match self.direction {
            SceneFadeDirection::Inclusive => hashed || self.threshold_above_zero,
            SceneFadeDirection::Keep => true,
            SceneFadeDirection::Complement => hashed,
        }
    }

    /// Whether the material has a fade band of its own, which a placement's band replaces.
    pub fn has_band(self) -> bool {
        self.band.is_some()
    }

    /// The authored point this material measures its fade distance from, if any.
    pub fn reference(self) -> Option<SceneFadeReference> {
        self.reference
    }

    /// Whether no fragment of a placement survives this material's fade. `band` is the
    /// placement's own fade band in metres, which replaces the material's; `distances`
    /// measures the placement's distances from the eye and keeps them for its other materials.
    pub fn empty(self, band: Option<[f32; 2]>, distances: &mut Distances<'_>) -> bool {
        if !self.can_empty() {
            return false;
        }
        let Some(band) = band.or(self.band) else {
            return false;
        };
        if !distances.eye.is_finite() {
            return false;
        }
        let measured = distances.measure(self.reference);
        // Away from the band's edges the estimate settles the coverage the interval
        // arithmetic below would compute; near them the intervals decide.
        if let Some(opacity) = measured
            .estimate
            .and_then(|estimate| estimate.coverage(self.directed(band)))
        {
            return self.covered(opacity);
        }
        self.empty_at(measured.span(), band)
    }

    fn empty_at(self, distance: Span, band: [f32; 2]) -> bool {
        if !distance.lo.is_finite() || !distance.hi.is_finite() {
            return false;
        }
        self.covered(distance_coverage(distance, self.directed(band)))
    }

    /// The band the coverage rules read: complement fades swap its sides.
    fn directed(self, band: [f32; 2]) -> [f32; 2] {
        if self.direction != SceneFadeDirection::Complement {
            return band;
        }
        if band[0] < 0.0 {
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
        }
    }

    /// Whether the fragment rules keep no fragment at `opacity`.
    fn covered(self, mut opacity: Span) -> bool {
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
}

#[cfg(test)]
impl Fade {
    /// `empty` for one placement measured on its own.
    pub fn empty_alone(self, object: worldrunner_render_plan::SceneObject, eye: Vec3) -> bool {
        let references: Vec<_> = self.reference.into_iter().collect();
        let anchors: Vec<_> = references
            .iter()
            .map(|&reference| Anchor::referenced(object.transform(), reference))
            .collect();
        let origin =
            crate::geometry::millimeters_to_meters(object.transform().position_millimeters);
        let mut distances = Distances::new(eye, origin.to_array(), &references, &anchors);
        self.empty(object.fade().map(fade_band_meters), &mut distances)
    }

    fn distance(self, object: worldrunner_render_plan::SceneObject, eye: Vec3) -> Span {
        self.reference
            .map_or_else(
                || {
                    let origin = crate::geometry::millimeters_to_meters(
                        object.transform().position_millimeters,
                    );
                    Anchor::origin(origin.to_array())
                },
                |reference| Anchor::referenced(object.transform(), reference),
            )
            .span(eye)
    }
}

/// One placement's distances from one eye, per fade reference, so the materials it draws
/// with measure each distance once. The eye-independent anchors come from the placement's
/// slot: its origin and one anchor per reference in `references`.
#[derive(Debug)]
pub(crate) struct Distances<'a> {
    eye: Vec3,
    origin: [f32; 3],
    references: &'a [SceneFadeReference],
    anchors: &'a [Anchor],
    measured_origin: Option<Measured>,
    measured_reference: Option<(SceneFadeReference, Measured)>,
}

impl<'a> Distances<'a> {
    pub fn new(
        eye: Vec3,
        origin: [f32; 3],
        references: &'a [SceneFadeReference],
        anchors: &'a [Anchor],
    ) -> Self {
        Self {
            eye,
            origin,
            references,
            anchors,
            measured_origin: None,
            measured_reference: None,
        }
    }

    fn measure(&mut self, reference: Option<SceneFadeReference>) -> &mut Measured {
        let eye = self.eye;
        let Some(reference) = reference else {
            let origin = self.origin;
            return self
                .measured_origin
                .get_or_insert_with(|| Measured::new(Anchor::origin(origin), eye));
        };
        if self
            .measured_reference
            .as_ref()
            .is_none_or(|(stored, _)| *stored != reference)
        {
            let index = self
                .references
                .iter()
                .position(|&stored| stored == reference)
                .expect("the slot holds an anchor for every reference its materials fade from");
            self.measured_reference = Some((reference, Measured::new(self.anchors[index], eye)));
        }
        &mut self.measured_reference.as_mut().expect("measured above").1
    }
}

/// The distance from one anchor: its estimate, and its interval once a band edge needs it.
#[derive(Debug)]
struct Measured {
    anchor: Anchor,
    eye: Vec3,
    estimate: Option<Estimate>,
    span: Option<Span>,
}

impl Measured {
    fn new(anchor: Anchor, eye: Vec3) -> Self {
        Self {
            anchor,
            eye,
            estimate: anchor.estimate(eye),
            span: None,
        }
    }

    fn span(&mut self) -> Span {
        let (anchor, eye) = (self.anchor, self.eye);
        *self.span.get_or_insert_with(|| anchor.span(eye))
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
pub(crate) mod reference;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod distance_tests;

#[cfg(test)]
mod estimate_tests;
