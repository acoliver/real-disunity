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

use worldrunner_disunity_runtime::GpuFlush;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::offscreen::{OffscreenError, OffscreenOperations, OffscreenState};
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{ComputeBufferId, GraphicsSnapshot, OffscreenWork};

use super::adapter::{OffscreenAdapter, build};
use super::assets::Assets;
use super::compute::{ComputeAdapter, GpuBuffers, Prepared};
use super::frame::{Adapted, Report};

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
        }
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
        let mut adapted = self
            .adapter
            .adapt(snapshot, assets, &mut self.state, &mut self.runtime);
        if adapted.frame.records.is_empty() {
            return Ok((None, adapted.report));
        }
        let operations = build(
            &mut self.state,
            &mut self.runtime,
            &self.arena,
            &mut adapted,
        )?;
        Ok((Some(operations), adapted.report))
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
        let mut adapted = self
            .adapter
            .adapt(snapshot, assets, &mut self.state, &mut self.runtime);
        let mut buffers = GpuBuffers::default();
        let result = self.run(snapshot, &mut adapted, &mut buffers);
        let outcome = result.map(|writes| FlushOutcome {
            writes,
            report: std::mem::take(&mut adapted.report),
        });
        // The submissions have completed or never happened, so the buffers can go.
        for handle in buffers.handles() {
            let _ = self.runtime.release_buffer(handle);
        }
        outcome
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
            self.submit(operations, prepared.as_ref())?;
        }
        adapted.report.built = built;
        self.read_back(buffers)
    }

    fn build_segment(
        &mut self,
        frame: &OffscreenFrame,
        segment: &[worldrunner_render_program::offscreen::frame::Record],
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
        let operations = build(&mut self.state, &mut self.runtime, &self.arena, &mut part)?;
        report.absorb_failures(part.report);
        Ok((operations, part.frame.records.len()))
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
        match self.compute.prepare(
            (&mut self.runtime, &self.state),
            snapshot,
            buffers,
            (*shader, *kernel, *groups, bindings),
        ) {
            Ok(prepared) => Some(prepared),
            Err(name) => {
                report.count(&format!("dispatch:{name}"));
                None
            }
        }
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
    fn read_back(&mut self, buffers: &GpuBuffers) -> Result<Vec<BufferWrite>, FlushError> {
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
            if bytes != original {
                writes.push(BufferWrite { id, bytes });
            }
        }
        Ok(writes)
    }
}
