use worldrunner_render_plan::{SceneObject, SceneTransform};

use super::{InstanceError, InstanceHandle, InstanceWrite, SceneGpu};

impl SceneGpu {
    /// Select the admitted slot before encoding. The host must retain its admission
    /// until submission completion before selecting the same slot again.
    pub fn select_frame_slot(&mut self, slot: usize) {
        let (buffer, group) = &self.frame_slots[slot];
        if let Some(pages) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
        {
            pages.borrow_mut().frame_slot = slot;
        }
        self.frame_buffer = buffer.clone();
        self.frame_group = group.clone();
    }

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
    /// Returns sparse edit ranges independently of any required recovery uploads.
    /// Cancelled replacement buffers are fully initialized before later draws.
    /// Use the same device and queue as upload. Drawing with pending bytes panics.
    pub fn flush_instances(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    ) -> Result<Vec<InstanceWrite>, worldrunner_render_gpu::submission::SubmitError> {
        let mut writes = Vec::new();
        let prepared = authority
            .prepare(device, |encoder, _| {
                writes = self.flush_instances_encoded(device, encoder)?;
                Ok::<_, worldrunner_render_gpu::submission::SubmitError>(())
            })
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        prepared.submit()?;
        Ok(writes)
    }
}

impl SceneGpu {
    /// Encodes growth copies followed by changed ranges before this frame's draws.
    /// Requires the active transaction encoder; cancellation discards publication.
    pub fn flush_instances_encoded(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Vec<InstanceWrite>, worldrunner_render_gpu::submission::TransactionError> {
        let mut context = worldrunner_render_post::PrepareContext::new(device, encoder)?;
        let writes = self.prepare_instances(&mut context);
        context.finish();
        Ok(writes)
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
        *self.drawing_frame.borrow_mut() = Some((context.publications(), frame));
        let published = self.frame.clone();
        let committed_group = self.committed_frame_group.clone();
        let group = self.frame_group.clone();
        context.publish(move || {
            published.set(Some(frame));
            *committed_group.borrow_mut() = group;
        });
        context.upload_buffer(&self.frame_buffer, 0, &frame.to_bytes());
    }
}
