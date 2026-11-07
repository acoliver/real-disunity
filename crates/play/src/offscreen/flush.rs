//! The player's GPU work: the frame path and the synchronous flush.
//!
//! [`OffscreenHost`] keeps what both paths share: the program runtime and its arena, the
//! offscreen state, and the adapters that load shaders, meshes, textures and compute kernels once.
//!
//! * [`OffscreenHost::frame`] adapts and builds a snapshot's records for the scene to encode
//!   before its own passes. Dispatches are not part of it; they are counted as left out.
//! * [`OffscreenHost::flush`] runs a snapshot's work to completion before it returns. It follows
//!   the records in call order, offscreen records and dispatches alike. Each run of records is
//!   built, recorded and submitted, then the dispatch that follows it is recorded and submitted
//!   with it. A last submission copies the compute buffers a dispatch bound back, and the call
//!   waits for that submission alone. The buffers whose bytes changed come out as
//!   [`BufferWrite`]s for the world to take with [`BufferSink`].
//!
//! Each run of records is its own submission because the buffer a run's constants share is
//! rewritten for the next run, and a submission fixes what its commands read.

use std::time::Instant;

use worldrunner_disunity_runtime::GpuFlush;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::unity::offscreen::{
    OffscreenError, OffscreenOperations, OffscreenState,
};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{ComputeBufferId, GraphicsSnapshot, OffscreenWork};

use super::adapter::{OffscreenAdapter, build};
use super::assets::Assets;
use super::compute::{ComputeAdapter, DispatchTrace, GpuBuffers, Prepared, nonzero_words};
use super::frame::{Adapted, Report};
use super::trace::WorkTrace;

/// Why a flush stopped. The world logs it once and the read that asked for the flush returns the
/// contents the CPU holds.
#[derive(Debug, thiserror::Error)]
pub enum FlushError {
    #[error("the offscreen operations were refused: {0}")]
    Offscreen(#[from] OffscreenError),
    #[error("the GPU work failed: {0}")]
    Program(#[from] ProgramError),
}

/// A compute buffer whose contents a dispatch changed, with all of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferWrite {
    pub id: ComputeBufferId,
    pub bytes: Vec<u8>,
}

/// What a flush did.
#[derive(Debug, Default)]
pub struct FlushOutcome {
    pub writes: Vec<BufferWrite>,
    pub report: Report,
}

/// Where the bytes a flush produced go.
pub trait BufferSink {
    /// Writes `bytes` into compute buffer `id` at `offset`.
    ///
    /// # Errors
    /// Fails with the world's reason when the range is not inside the buffer.
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String>;
}

impl BufferSink for GpuFlush<'_> {
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String> {
        self.write_compute_buffer(id.to_bits(), offset, bytes)
            .map_err(|error| error.to_string())
    }
}

impl BufferSink for World {
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String> {
        let offset = usize::try_from(offset).map_err(|error| error.to_string())?;
        self.set_compute_buffer_data(id, offset, bytes)
            .map_err(|error| error.to_string())
    }
}

impl FlushOutcome {
    /// Gives every write to `sink`.
    ///
    /// # Errors
    /// Fails with the first write the sink refuses.
    pub fn apply(&self, sink: &mut impl BufferSink) -> Result<(), String> {
        self.writes
            .iter()
            .try_for_each(|write| sink.write(write.id, 0, &write.bytes))
    }
}

/// The player's side of a device: its runtime, arena, state and adapters.
pub struct OffscreenHost {
    runtime: ProgramRuntime,
    arena: ProgramArena,
    state: OffscreenState,
    adapter: OffscreenAdapter,
    compute: ComputeAdapter,
    traces: Vec<WorkTrace>,
}

impl OffscreenHost {
    /// Bytes of GPU storage the runtime may hold.
    #[must_use]
    pub fn storage_budget(&self) -> u64 {
        self.runtime.budget().0
    }

    /// Bytes of GPU storage the runtime holds now: the render textures and uploads it keeps, and
    /// storage released but possibly still in use.
    #[must_use]
    pub fn live_bytes(&self) -> u64 {
        self.runtime.live_bytes()
    }

    #[must_use]
    pub fn new(
        runtime: ProgramRuntime,
        arena: ProgramArena,
        adapter: OffscreenAdapter,
        compute: ComputeAdapter,
    ) -> Self {
        let state = OffscreenState::new(&runtime);
        Self {
            runtime,
            arena,
            state,
            adapter,
            compute,
            traces: Vec::new(),
        }
    }

    /// The traces of the frames and flushes since the last call, in order, whether they were
    /// refused or not.
    pub fn take_traces(&mut self) -> Vec<WorkTrace> {
        std::mem::take(&mut self.traces)
    }

    /// Notes one piece of work: its report's counts, the time its adaptation took and the bytes
    /// the runtime held before it and holds now.
    fn note(
        &mut self,
        (kind, live_before): (&'static str, u64),
        report: &Report,
        error: Option<String>,
    ) {
        self.traces.push(WorkTrace {
            kind,
            records: report.records,
            built: report.built,
            camera_renders: report.camera_draws.len(),
            drawn: report.camera_draws.iter().map(|c| c.drawn).sum(),
            left_out: report
                .camera_draws
                .iter()
                .map(|c| c.unmatched + c.missing_texture + c.missing_channel)
                .sum(),
            live_before,
            live_after: self.runtime.live_bytes(),
            phases: report.phases,
            error,
        });
    }

    /// Sets the most bytes one buffer of the frame's constants may hold. A frame with more is
    /// uploaded in several buffers.
    pub fn set_constants_limit(&mut self, bytes: u64) {
        self.state.set_constants_limit(bytes);
    }

    /// The runtime, for a caller that draws with the same device.
    pub fn runtime_mut(&mut self) -> &mut ProgramRuntime {
        &mut self.runtime
    }

    /// Adapts and builds the records of `snapshot` for the scene's next frame. A frame with no
    /// records gives `None`.
    ///
    /// # Errors
    /// Fails with the error of the records that are left when they still do not build together.
    pub fn frame(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
    ) -> Result<(Option<OffscreenOperations>, Report), OffscreenError> {
        let live_before = self.runtime.live_bytes();
        let started = Instant::now();
        let mut adapted = self
            .adapter
            .adapt(snapshot, assets, &mut self.state, &mut self.runtime);
        adapted.report.phases.adapt = started.elapsed();
        if adapted.frame.records.is_empty() {
            self.note(("frame", live_before), &adapted.report, None);
            return Ok((None, adapted.report));
        }
        let built = build(
            &mut self.state,
            &mut self.runtime,
            &self.arena,
            &mut adapted,
        );
        let error = built.as_ref().err().map(ToString::to_string);
        self.note(("frame", live_before), &adapted.report, error);
        Ok((Some(built?), adapted.report))
    }

    /// Runs the work of `snapshot` to completion and returns the compute buffers it changed.
    ///
    /// # Errors
    /// Fails when the records do not build together or the GPU refuses the work. Nothing is
    /// written then.
    pub fn flush(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
    ) -> Result<FlushOutcome, FlushError> {
        let live_before = self.runtime.live_bytes();
        let started = Instant::now();
        let mut adapted = self
            .adapter
            .adapt(snapshot, assets, &mut self.state, &mut self.runtime);
        adapted.report.phases.adapt = started.elapsed();
        let mut buffers = GpuBuffers::default();
        let result = self.run(snapshot, &mut adapted, &mut buffers);
        let error = result.as_ref().err().map(ToString::to_string);
        // The buffers can go once the GPU has finished with them, and the next flush then
        // allocates on top of what is live, not of what this one released.
        for handle in buffers.handles() {
            let _ = self.runtime.release_buffer(handle);
        }
        self.settle();
        self.note(("flush", live_before), &adapted.report, error);
        result.map(|writes| FlushOutcome {
            writes,
            report: std::mem::take(&mut adapted.report),
        })
    }

    /// Waits for the GPU to finish every submission so far and frees the storage released
    /// meanwhile. The runtime counts released storage against its budget until the GPU is done
    /// with it, so a host that does not wait lets the next piece of work allocate on top of the
    /// last one's temporaries.
    pub fn settle(&mut self) {
        let waited = self.runtime.device().poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        // A device that cannot be waited on has nothing further to collect.
        if waited.is_ok() {
            self.runtime.collect();
        }
    }

    fn run(
        &mut self,
        snapshot: &GraphicsSnapshot,
        adapted: &mut Adapted,
        buffers: &mut GpuBuffers,
    ) -> Result<Vec<BufferWrite>, FlushError> {
        let placements = std::mem::take(&mut adapted.dispatches);
        let records = std::mem::take(&mut adapted.frame.records);
        let mut built = 0;
        let mut from = 0;
        for (index, until) in placements
            .iter()
            .map(|placed| placed.after)
            .chain(std::iter::once(records.len()))
            .enumerate()
        {
            let segment = &records[from..until];
            from = until;
            // The first build also creates the render textures the dispatches bind.
            let operations = if segment.is_empty() && (index > 0 || placements.is_empty()) {
                None
            } else {
                let (operations, kept) =
                    self.build_segment(&adapted.frame, segment, &mut adapted.report)?;
                built += kept;
                Some(operations)
            };
            let prepared = placements.get(index).and_then(|placed| {
                self.prepare(snapshot, placed.work, buffers, &mut adapted.report)
            });
            let submitted = Instant::now();
            self.submit(operations, prepared.as_ref())?;
            adapted.report.phases.submit += submitted.elapsed();
            if prepared.is_some()
                && let Some(trace) = adapted.report.traces.last_mut()
            {
                "ran".clone_into(&mut trace.outcome);
            }
        }
        adapted.report.built = built;
        self.read_back(buffers, &mut adapted.report.traces)
    }

    fn build_segment(
        &mut self,
        frame: &OffscreenFrame,
        segment: &[worldrunner_render_program::unity::offscreen::frame::Record],
        report: &mut Report,
    ) -> Result<(OffscreenOperations, usize), FlushError> {
        let mut part = Adapted {
            frame: OffscreenFrame {
                records: segment.to_vec(),
                ..frame.clone()
            },
            report: Report::default(),
            dispatches: Vec::new(),
        };
        let built = build(&mut self.state, &mut self.runtime, &self.arena, &mut part);
        // A refused segment still tells how many refusals and probes it cost and how long.
        let kept = part.frame.records.len();
        report.absorb_failures(part.report);
        Ok((built?, kept))
    }

    fn prepare(
        &mut self,
        snapshot: &GraphicsSnapshot,
        work: usize,
        buffers: &mut GpuBuffers,
        report: &mut Report,
    ) -> Option<Prepared> {
        let OffscreenWork::Dispatch {
            shader,
            kernel,
            groups,
            bindings,
        } = &snapshot.work[work]
        else {
            return None;
        };
        let mut trace = DispatchTrace::default();
        let prepared = self.compute.prepare(
            (&mut self.runtime, &self.state),
            snapshot,
            buffers,
            (*shader, *kernel, *groups, bindings),
            &mut trace,
        );
        for texture in &mut trace.textures {
            for camera in report
                .camera_draws
                .iter()
                .filter(|camera| Some(camera.target) == texture.texture)
            {
                texture.camera_renders += 1;
                texture.camera_draws += camera.drawn;
                texture.camera_unmatched += camera.unmatched;
                texture.camera_missing_texture += camera.missing_texture;
                texture.camera_missing_channel += camera.missing_channel;
            }
        }
        let outcome = match prepared {
            Ok(prepared) => Some(prepared),
            Err(name) => {
                report.count(&format!("dispatch:{name}"));
                name.clone_into(&mut trace.outcome);
                None
            }
        };
        report.traces.push(trace);
        outcome
    }

    /// Records the operations and then the dispatch into one frame and submits it.
    fn submit(
        &mut self,
        operations: Option<OffscreenOperations>,
        dispatch: Option<&Prepared>,
    ) -> Result<(), FlushError> {
        if operations.is_none() && dispatch.is_none() {
            return Ok(());
        }
        let authority = self.runtime.authority().clone();
        let device = self.runtime.device().clone();
        let mut frame = authority.begin(&device).map_err(ProgramError::from)?;
        // The admitted operations stay alive until the frame is submitted.
        let admitted = operations
            .map(|operations| operations.admit(&frame))
            .transpose()?;
        if let Some(admitted) = &admitted {
            admitted
                .encode(None, &mut frame)
                .map_err(OffscreenError::from)?;
        }
        if let Some(dispatch) = dispatch {
            self.runtime.dispatch(&mut frame, &dispatch.request())?;
        }
        frame.finish().submit().map_err(ProgramError::from)?;
        Ok(())
    }

    /// Copies every bound compute buffer back with one more submission, waits for it alone and
    /// returns the buffers whose bytes changed.
    fn read_back(
        &mut self,
        buffers: &GpuBuffers,
        traces: &mut [DispatchTrace],
    ) -> Result<Vec<BufferWrite>, FlushError> {
        let bound: Vec<_> = buffers.bound().collect();
        if bound.is_empty() {
            return Ok(Vec::new());
        }
        let authority = self.runtime.authority().clone();
        let device = self.runtime.device().clone();
        let mut frame = authority.begin(&device).map_err(ProgramError::from)?;
        let mut pending = Vec::new();
        for (id, handle, original) in &bound {
            pending.push((
                *id,
                *original,
                self.runtime.read_buffer(&mut frame, *handle)?,
            ));
        }
        let submission = frame.finish().submit().map_err(ProgramError::from)?;
        let mut writes = Vec::new();
        for (id, original, read) in pending {
            let bytes = read.resolve(submission.clone())?;
            for trace in traces.iter_mut().filter(|trace| trace.outcome == "ran") {
                for buffer in trace
                    .buffers
                    .iter_mut()
                    .filter(|buffer| buffer.id == Some(id))
                {
                    buffer.nonzero_after = Some(nonzero_words(&bytes));
                    trace.read_back = true;
                }
            }
            if bytes != original {
                writes.push(BufferWrite { id, bytes });
            }
        }
        Ok(writes)
    }
}
