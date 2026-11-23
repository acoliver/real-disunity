//! Test-only accessors of this module's types.

use super::*;

impl SceneGpu {
    /// The group the object draws bind at set 0, for tests that record the objects into pass
    /// commands of their own.
    pub(crate) fn drawing_frame_group_for_test(&self) -> wgpu::BindGroup {
        self.drawing_frame_group()
    }

    /// The keys, as indices into `PipelineKey::ALL` after `mask`, that the camera view's
    /// selection draws now. It runs the walk a pass records and keeps each draw's pipeline.
    pub(crate) fn pipeline_keys_drawn(&self, mask: usize) -> std::collections::BTreeSet<usize> {
        let frame = self.drawing_uniform();
        let selection = self.select_objects(None, &frame);
        let mut sink = KeySink {
            mask,
            keys: std::collections::BTreeSet::new(),
        };
        self.record_selection(&mut sink, (None, &frame), &selection.ranges);
        sink.keys
    }

    /// [`draw_objects_prepared`](Self::draw_objects_prepared), sending pipeline index `i` to the
    /// pipeline of key `i ^ mask`.
    pub(crate) fn draw_objects_prepared_masked<P: PassCommands + ?Sized>(
        &self,
        pass: &mut P,
        frame_group: &wgpu::BindGroup,
        context: &Context,
        mask: usize,
    ) -> DrawnObjects {
        let view = self.begin_objects(pass, frame_group, None);
        let mut sink = PreparedSink::new(pass, &self.objects, context).with_mask(mask);
        let objects = self.record_selection(&mut sink, (None, &view.frame), &view.selection.ranges);
        sink.finish();
        self.end_objects(view, None, objects)
    }
}
