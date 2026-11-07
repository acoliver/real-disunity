use super::{PrepareContext, Prepared, Streaming};
use crate::{draw::TerrainStreamingStatistics, frame::FrameUniform};

impl Streaming {
    pub(super) fn prepare_frozen(
        &self,
        context: &PrepareContext<'_>,
        frame: &FrameUniform,
        viewport: [u32; 2],
    ) {
        let residents = self.committed.borrow().clone();
        let current: Vec<_> = residents.iter().map(|(key, _)| *key).collect();
        self.residency.observe(current.iter().copied(), false);
        let observation = crate::terrain::streaming::observe(
            self.scene.terrain().expect("terrain"),
            &current,
            frame.selection_view_projection(),
            viewport,
        );
        *self.drawing.borrow_mut() = Some(Prepared {
            owner: context.publications(),
            matrix: frame.selection_view_projection(),
            viewport,
            residents,
            observation,
            #[cfg(test)]
            uploads: 0,
        });
    }

    pub(in crate::draw) fn drawing_statistics(&self) -> TerrainStreamingStatistics {
        let drawing = self.drawing.borrow();
        let prepared = drawing
            .as_ref()
            .expect("prepare streaming before owning draw");
        let current = worldrunner_render_gpu::submission::Publications::current()
            .expect("owning composition transaction");
        assert!(current.same_frame(&prepared.owner));
        let committed = self.committed.borrow();
        let residents = if current.same_transaction(&prepared.owner) {
            &prepared.residents
        } else {
            &*committed
        };
        let drawn: Vec<_> = residents.iter().map(|(key, _)| *key).collect();
        crate::draw::geometry_statistics::measure(&drawn, prepared.observation)
    }
}
