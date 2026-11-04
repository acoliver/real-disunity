use super::ShadowFeature;

impl ShadowFeature<'_> {
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
