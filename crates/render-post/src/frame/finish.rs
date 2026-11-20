//! Recording what a context staged, in staging order, once every preparation succeeded.
use super::{FrameError, PrepareContext, Staged, prepared};

impl PrepareContext<'_> {
    /// Publishes staged copies and dispatches only after all fallible preparations succeed.
    ///
    /// # Panics
    /// When the owner of staged prepared commands refuses to record them. Only the features'
    /// preparation of a frame stages prepared commands, and the frame finishes it reporting the
    /// refusal instead.
    pub fn finish(self) -> Vec<wgpu::Buffer> {
        self.finish_preparation()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Publishes staged copies, dispatches and prepared commands, in staging order, only after
    /// all fallible preparations succeed.
    ///
    /// # Errors
    /// [`FrameError::PreparedRefused`] when the owner of staged prepared commands refuses to
    /// record them; what was staged before them is recorded and the frame must be cancelled.
    pub(crate) fn finish_preparation(self) -> Result<Vec<wgpu::Buffer>, FrameError> {
        let mut retained = Vec::with_capacity(self.staged.len());
        for staged in self.staged {
            match staged {
                Staged::Copy {
                    source,
                    destination,
                    bytes,
                } => {
                    self.encoder.copy_buffer_to_buffer(
                        &source.0,
                        source.1,
                        &destination.0,
                        destination.1,
                        bytes,
                    );
                    retained.push(source.0);
                }
                Staged::Dispatch {
                    label,
                    pipeline,
                    group,
                    workgroups,
                } => {
                    let mut pass = self
                        .encoder
                        .begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some(label),
                            timestamp_writes: None,
                        });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
                }
                Staged::Prepared { label, commands } => {
                    prepared::record(self.encoder, label, commands.as_ref())?;
                }
            }
        }
        Ok(retained)
    }
}
