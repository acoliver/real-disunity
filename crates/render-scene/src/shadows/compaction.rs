use super::ShadowFeature;

impl ShadowFeature<'_> {
    /// Selects the camera and every cascade that draws this frame at once, so their
    /// selections build in parallel before the gathers below.
    pub(super) fn select_views(&self, context: &worldrunner_render_post::PrepareContext<'_>) {
        let mut views = vec![None];
        if !self.cascades.no_receivers {
            views.extend(
                self.cascades
                    .matrices
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| self.refresh[*index])
                    .map(|(index, &matrix)| {
                        Some((u32::try_from(index).expect("four cascades"), matrix))
                    }),
            );
        }
        self.gpu.select_stale_views(context, &views, &self.frame);
    }

    /// Gathers the instances of every cascade that draws this frame.
    pub(super) fn prepare_cascade_compaction(
        &self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) {
        if self.cascades.no_receivers {
            return;
        }
        for (index, &matrix) in self.cascades.matrices.iter().enumerate() {
            if self.refresh[index] {
                let index = u32::try_from(index).expect("four cascades");
                self.gpu
                    .prepare_view_compaction(context, Some((index, matrix)), &self.frame);
            }
        }
    }
}
