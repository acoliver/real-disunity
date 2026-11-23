//! Test-only accessors of this module's types.

use super::*;

impl SceneGpu {
    /// Records the view's objects with render pipelines the caller built, one for each pipeline
    /// index, and a bind group for each material (see
    /// [`PreparedObjects::material_groups`](crate::object::prepared::PreparedObjects::material_groups)).
    /// Only tests draw this way; the scene's passes use
    /// [`draw_objects_prepared`](Self::draw_objects_prepared).
    pub(crate) fn draw_objects(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        (pipelines, materials): (
            &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
            &[BindGroup],
        ),
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> DrawnObjects {
        let view = self.begin_objects(pass, frame_group, shadow_matrix);
        let mut sink = FixedSink::new(pass, pipelines, materials);
        let objects = self.record_selection(
            &mut sink,
            (shadow_matrix, &view.frame),
            &view.selection.ranges,
        );
        self.end_objects(view, shadow_matrix, objects)
    }
}
