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
        self.release_geometry(&residents);
        let current: Vec<_> = residents.iter().map(|(key, _)| *key).collect();
        self.residency.observe(current.iter().copied(), false);
        let observation = crate::terrain::streaming::observe(
            self.scene.terrain().expect("terrain"),
            &self.height_bounds,
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

    pub(in crate::draw) fn drawing_statistics(
        &self,
        matrix: glam::Mat4,
        submitted: bool,
    ) -> TerrainStreamingStatistics {
        let drawing = self.drawing.borrow();
        let prepared = drawing
            .as_ref()
            .expect("prepare streaming before owning draw");
        let current = worldrunner_render_gpu::submission::Publications::current()
            .expect("owning composition transaction");
        assert!(current.same_frame(&prepared.owner));
        let residents = &prepared.residents;
        let keys: Vec<_> = residents.iter().map(|(key, _)| *key).collect();
        let drawn: Vec<_> = if submitted {
            super::culling::visible(
                self.scene.terrain().expect("terrain"),
                &self.height_bounds,
                matrix,
                residents,
            )
            .into_iter()
            .map(|(key, _)| *key)
            .collect()
        } else {
            Vec::new()
        };
        crate::draw::geometry_statistics::measure(&keys, &drawn, prepared.observation)
    }
}
