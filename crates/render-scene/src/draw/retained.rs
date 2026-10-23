use worldrunner_render_plan::{SceneObject, SceneTransform};

use super::{InstanceError, InstanceHandle, InstanceWrite, SceneGpu};

impl SceneGpu {
    /// Adds a placement referencing this scene's existing mesh/material tables.
    /// Queue writes are deferred until [`Self::flush_instances`].
    pub fn add_instance(
        &mut self,
        object: SceneObject,
        visible: bool,
    ) -> Result<InstanceHandle, InstanceError> {
        self.instances.add(object, visible)
    }

    /// Enumerates live handles in slot order, initially the input object order.
    /// Removed slots are reused lowest-first, with a new generation.
    pub fn instance_handles(&self) -> impl Iterator<Item = InstanceHandle> + '_ {
        self.instances.handles()
    }

    /// Reads the current CPU placement and visibility, including pending edits.
    pub fn instance(&self, handle: InstanceHandle) -> Result<(SceneObject, bool), InstanceError> {
        self.instances.get(handle)
    }

    /// Removes a placement without moving surviving batch slots or rewriting bytes.
    pub fn remove_instance(&mut self, handle: InstanceHandle) -> Result<(), InstanceError> {
        self.instances.remove(handle)
    }

    /// Replaces a validated transform. Repeated edits coalesce at frame flush.
    pub fn set_instance_transform(
        &mut self,
        handle: InstanceHandle,
        transform: SceneTransform,
    ) -> Result<(), InstanceError> {
        self.instances.set_transform(handle, transform)
    }

    /// Changes draw membership without a GPU write, including shadow membership.
    pub fn set_instance_visible(
        &mut self,
        handle: InstanceHandle,
        visible: bool,
    ) -> Result<(), InstanceError> {
        self.instances.set_visible(handle, visible)
    }

    /// Writes only changed contiguous instance ranges before recording a frame.
    /// Returns the exact queue-write ranges; an unchanged scene returns no ranges.
    /// Growing a batch copies its existing GPU buffer instead of reuploading it.
    /// Use the same device and queue as upload. Drawing with pending bytes panics.
    pub fn flush_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<InstanceWrite> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("instance uploads"),
        });
        let writes = self.flush_instances_encoded(device, &mut encoder);
        if !writes.is_empty() {
            queue.submit([encoder.finish()]);
        }
        writes
    }
}

impl SceneGpu {
    /// Encodes growth copies followed by changed ranges before this frame's draws.
    /// The caller must submit this encoder before encoding another frame.
    pub fn flush_instances_encoded(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Vec<InstanceWrite> {
        let mut context = worldrunner_render_post::PrepareContext::new(device, encoder);
        let writes = self.prepare_instances(&mut context);
        context.finish();
        writes
    }

    /// Commits sparse edits using the restricted upload context.
    /// Call only after all fallible frame preparation succeeds, then finish the context.
    pub fn prepare_instances(
        &mut self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Vec<InstanceWrite> {
        let mut writes = Vec::new();
        for (&(_, mesh, material), batch) in &mut self.instances.batches {
            for instances in crate::instances::batch::flush_prepared(batch, context) {
                writes.push(InstanceWrite {
                    mesh,
                    material,
                    instances,
                });
            }
        }
        writes
    }
}

impl SceneGpu {
    #[cfg(test)]
    pub(crate) fn current_frame(&self) -> Option<crate::frame::FrameUniform> {
        self.frame.get()
    }

    #[cfg(test)]
    pub(crate) fn frame_buffer(&self) -> &wgpu::Buffer {
        &self.frame_buffer
    }

    /// Publishes CPU selection state and stages its matching encoder-ordered uniform.
    /// Call only within the scheduler's infallible commit phase.
    pub fn commit_frame(
        &self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
        frame: crate::frame::FrameUniform,
    ) {
        self.frame.set(Some(frame));
        context.upload_buffer(&self.frame_buffer, 0, &frame.to_bytes());
    }
}
