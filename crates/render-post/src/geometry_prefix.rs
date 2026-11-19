use crate::{AttachmentView, FrameError, FramePlan, PassDeclaration, PassEntry};
use worldrunner_render_gpu::submission::Publications;

pub(crate) struct GeometryPrefix {
    frame: Publications,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
}
impl GeometryPrefix {
    pub(crate) fn new(frame: Publications, plan: &FramePlan<'_>) -> Self {
        Self {
            frame,
            passes: plan.passes.to_vec(),
            views: plan.views.to_vec(),
        }
    }

    pub(crate) fn completed_entries(
        self,
        frame: &Publications,
        plan: &FramePlan<'_>,
    ) -> Result<Vec<PassEntry>, FrameError> {
        if !self.frame.same_frame(frame) {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for produced in self.passes {
            // A depth-only consumer need not include metadata in its suffix plan.
            let Some(pass) = plan
                .passes
                .iter()
                .find(|pass| pass.entry() == produced.entry())
            else {
                continue;
            };
            if pass.color != produced.color
                || pass.additional_colors != produced.additional_colors
                || pass.depth != produced.depth
            {
                return Err(FrameError::AttachmentContract(pass.entry()));
            }
            for attachment in pass
                .color
                .iter()
                .chain(&pass.additional_colors)
                .chain(&pass.depth)
            {
                let old = self
                    .views
                    .iter()
                    .find(|view| view.resource() == attachment.resource);
                let new = plan
                    .views
                    .iter()
                    .find(|view| view.resource() == attachment.resource);
                if !matches!((old, new), (Some(old), Some(new)) if old.texture() == new.texture()) {
                    return Err(FrameError::AttachmentView(attachment.resource));
                }
            }
            entries.push(pass.entry());
        }
        Ok(entries)
    }
}

impl<F: super::StageFactory> super::PostChain<F> {
    pub(super) fn completed_geometry_prefix(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        plan: &FramePlan<'_>,
    ) -> Result<Vec<PassEntry>, FrameError> {
        let frame = Publications::for_encoder(encoder)?;
        self.geometry_prefix
            .take()
            .map(|prefix| prefix.completed_entries(&frame, plan))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

/// The retained features a geometry prefix does not run, and where each one sat.
pub(super) struct Parked {
    /// For each registered feature in order, whether it was parked.
    order: Vec<bool>,
    features: Vec<crate::frame::RegisteredFeature>,
}

impl<F: super::StageFactory> super::PostChain<F> {
    /// Registers a feature that takes part in geometry prefixes as well as in the frames that
    /// complete them.
    ///
    /// Such a feature draws only in entries a prefix completes, such as the game's passes after
    /// the depth prepass. Its `plan` is called for each frame as for any retained feature, and
    /// for each geometry prefix through [`Self::plan_prefix_features`]; the prefix prepares and
    /// draws it. A frame that completes the prefix neither prepares nor draws it again when the
    /// prefix completed every entry it declares, and runs it as usual otherwise.
    ///
    /// # Errors
    /// As [`Self::register`].
    pub fn register_prefix(
        &mut self,
        device: &wgpu::Device,
        feature: Box<dyn crate::FrameFeature>,
    ) -> Result<crate::FeatureId, FrameError> {
        let id = self.register(device, feature)?;
        self.features[id.0].prefix = true;
        Ok(id)
    }

    /// Whether any registered feature takes part in geometry prefixes.
    pub fn has_prefix_features(&self) -> bool {
        self.features.iter().any(|feature| feature.prefix)
    }

    /// Plans the prefix participants into the pass list of the next geometry prefix, after the
    /// prefix's own passes, and keeps their declarations for
    /// [`Self::encode_geometry_prefix`]. The caller orders the list.
    ///
    /// # Errors
    /// The first participant's planning error.
    pub fn plan_prefix_features(
        &mut self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        self.planned_prefix = None;
        let mut planned = Vec::new();
        for (index, feature) in self.features.iter().enumerate() {
            if feature.prefix {
                planned.push((index, feature.source.plan(passes, views)?));
            }
        }
        self.planned_prefix = Some(planned);
        Ok(())
    }

    /// Takes out every retained feature but the prefix participants planned for this prefix,
    /// which keep the declarations they were planned with.
    pub(super) fn park_non_prefix_features(&mut self) -> Parked {
        let planned = self.planned_prefix.take().unwrap_or_default();
        let mut order = Vec::new();
        let mut kept = Vec::new();
        let mut features = Vec::new();
        for (index, mut feature) in std::mem::take(&mut self.features).into_iter().enumerate() {
            if let Some((_, declaration)) = planned.iter().find(|(planned, _)| *planned == index) {
                feature.declaration = declaration.clone();
                kept.push(feature);
                order.push(false);
            } else {
                features.push(feature);
                order.push(true);
            }
        }
        self.features = kept;
        Parked { order, features }
    }

    pub(super) fn restore_parked_features(&mut self, parked: Parked) {
        let mut kept = std::mem::take(&mut self.features).into_iter();
        let mut features = parked.features.into_iter();
        self.features = parked
            .order
            .into_iter()
            .map(|was_parked| {
                if was_parked {
                    features.next()
                } else {
                    kept.next()
                }
                .expect("one feature per registration")
            })
            .collect();
    }

    /// The retained features a frame prepares and draws: every one but the prefix participants
    /// whose entries a geometry prefix of the same frame completed.
    pub(super) fn active_features<'a>(
        &'a mut self,
        completed: &'a [PassEntry],
    ) -> impl Iterator<Item = &'a mut crate::frame::RegisteredFeature> {
        self.features
            .iter_mut()
            .filter(move |feature| !feature.completed_in(completed))
    }
}
