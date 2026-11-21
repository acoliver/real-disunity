//! Recording what a context staged, in staging order, once every preparation succeeded.
//!
//! Compute work staged one after another, dispatches and prepared commands with a compute form,
//! is recorded into one compute pass, which the next copy or prepared commands without a compute
//! form end. wgpu-core encodes every compute pass into two command buffers that it holds until the
//! frame submits, and Metal's queue refuses the one after 4096, so a pass for each dispatch limits
//! how much a frame can prepare. Inside one pass wgpu-core gives each dispatch its own usage scope
//! and the barriers it needs after the dispatches before it, so a dispatch still reads what the
//! one before it wrote.
use super::{FrameError, PrepareContext, PreparedCompute, Staged, prepared};

/// The label of the compute pass a run of staged compute work shares. Each staged item in it is
/// named by a debug group of its own label.
const COMPUTE_RUN: &str = "prepared compute";

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
    /// all fallible preparations succeed. Compute work staged one after another shares one
    /// compute pass.
    ///
    /// # Errors
    /// [`FrameError::PreparedRefused`] when the owner of staged prepared commands refuses to
    /// record them; what was staged before them is recorded and the frame must be cancelled.
    pub(crate) fn finish_preparation(self) -> Result<Vec<wgpu::Buffer>, FrameError> {
        let mut retained = Vec::with_capacity(self.staged.len());
        // The open pass locks the encoder until it is dropped, which ends it.
        let mut run: Option<wgpu::ComputePass<'static>> = None;
        for staged in self.staged {
            let compute = match &staged {
                Staged::Copy {
                    source,
                    destination,
                    bytes,
                } => {
                    run = None;
                    self.encoder.copy_buffer_to_buffer(
                        &source.0,
                        source.1,
                        &destination.0,
                        destination.1,
                        *bytes,
                    );
                    retained.push(source.0.clone());
                    continue;
                }
                Staged::TextureCopy {
                    source,
                    destination,
                    size,
                } => {
                    run = None;
                    self.encoder.copy_texture_to_texture(
                        super::prepare::borrowed(source),
                        super::prepare::borrowed(destination),
                        *size,
                    );
                    continue;
                }
                Staged::Dispatch {
                    label,
                    pipeline,
                    group,
                    workgroups,
                } => Compute::Dispatch {
                    label,
                    pipeline,
                    group,
                    workgroups: *workgroups,
                },
                Staged::Prepared { label, commands } => {
                    let Some(compute) = commands.compute() else {
                        run = None;
                        prepared::record(self.encoder, label, commands.as_ref())?;
                        continue;
                    };
                    Compute::Prepared {
                        label,
                        commands: compute,
                    }
                }
            };
            let pass = run.get_or_insert_with(|| {
                self.encoder
                    .begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(COMPUTE_RUN),
                        timestamp_writes: None,
                    })
                    .forget_lifetime()
            });
            compute.record(pass)?;
        }
        drop(run);
        Ok(retained)
    }
}

/// Staged work that shares a compute pass with the compute work staged next to it.
enum Compute<'a> {
    Dispatch {
        label: &'static str,
        pipeline: &'a wgpu::ComputePipeline,
        group: &'a wgpu::BindGroup,
        workgroups: [u32; 3],
    },
    Prepared {
        label: &'static str,
        commands: &'a dyn PreparedCompute,
    },
}

impl Compute<'_> {
    /// Records the work into `pass`, named by a debug group of its label.
    fn record(&self, pass: &mut wgpu::ComputePass<'_>) -> Result<(), FrameError> {
        match *self {
            Self::Dispatch {
                label,
                pipeline,
                group,
                workgroups: [x, y, z],
            } => {
                pass.push_debug_group(label);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.dispatch_workgroups(x, y, z);
                pass.pop_debug_group();
                Ok(())
            }
            Self::Prepared { label, commands } => prepared::record_in_pass(pass, label, commands),
        }
    }
}
