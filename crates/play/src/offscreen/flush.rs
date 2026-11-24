//! The player's GPU work: the frame path and the synchronous flush.
//!
//! [`OffscreenHost`] keeps what both paths share: the program runtime and its arena, the
//! offscreen state, and the adapters that load shaders, meshes, textures and compute kernels once.
//!
//! * [`OffscreenHost::frame`] adapts and builds a snapshot's records for the scene to encode
//!   before its own passes. Dispatches are not part of it; they are counted as left out.
//!   The compute buffers its materials bind are uploaded first and kept until the next frame.
//! * [`OffscreenHost::flush`] runs a snapshot's work to completion before it returns. It follows
//!   the records in call order, offscreen records and dispatches alike. Each run of records is
//!   built, recorded and submitted, then the dispatch that follows it is recorded and submitted
//!   with it. A last submission copies the compute buffers a dispatch bound back, and the call
//!   waits for that submission alone. The buffers whose bytes changed come out as
//!   [`BufferWrite`]s for the world to take with [`BufferSink`]. The compute buffers the
//!   materials of the work bind are uploaded before the first run is built, and a dispatch that
//!   binds one of them shares its upload.
//!
//! A pixel read is placed like a dispatch: the records before it are submitted with a copy of the
//! render texture recorded last, and the copy's rectangle comes out as a [`PixelAnswer`] for the
//! world to take with [`BufferSink`] (see the `pixels` module).
//!
//! Each run of records is its own submission because the buffer a run's constants share is
//! rewritten for the next run, and a submission fixes what its commands read. A run that would
//! plan more draws than the host's [draw budget](DRAW_BUDGET) is built and submitted in parts,
//! each submission of its own, so the draws one part planned are dropped before the next is
//! planned.

use std::ops::Range;
use std::time::Instant;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime, TextureHandle};
use worldrunner_render_program::unity::offscreen::frame::{OffscreenFrame, Record};
use worldrunner_render_program::unity::offscreen::{
    CameraDraws, ListDraws, OffscreenError, OffscreenOperations, OffscreenState,
};
use worldrunner_world_simulation::graphics::{ComputeBufferId, GraphicsSnapshot, OffscreenWork};

use super::adapter::{OffscreenAdapter, Settling, build};
use super::assets::Assets;
use super::compute::{ComputeAdapter, DispatchTrace, GpuBuffers, Prepared, nonzero_words};
use super::frame::{Adapted, Reclaimed, Report, is_test_render};
use super::ids;
use super::pass_notes;
use super::pixels::{self, Copied, PixelAnswer, Wanted};
use super::probe::{BeforeDispatch, Probe};
#[cfg(test)]
use super::screen::ScreenImage;
use super::screen::writes_screen;
use super::sink::BufferSink;
use super::taps;
use super::trace::WorkTrace;

mod parts;
mod screen_view;
mod ui_read;

pub use ui_read::UiRead;

/// The most draws one build of a flush plans unless the host sets another.
///
/// A planned draw holds its constants, its bindings and its texture views until the build that
/// planned it is submitted, about 13 KB of memory for a draw of a game that binds several
/// textures. A frame of hundreds of camera renders of thousands of draws each would hold every
/// one of them at once. A flush plans this many, submits them and drops them before it plans
/// more: 16,384 draws take about 200 MB, and a part is large enough that the cost of one more
/// submission is small against what it draws.
pub const DRAW_BUDGET: usize = 16_384;

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
    /// The answers to the pixel reads the flush made, in call order.
    pub pixels: Vec<PixelAnswer>,
    pub report: Report,
}

/// What running a flush's work produced.
struct Produced {
    writes: Vec<BufferWrite>,
    pixels: Vec<PixelAnswer>,
}

impl FlushOutcome {
    /// Gives every write and every pixel answer to `sink`.
    ///
    /// # Errors
    /// Fails with the first write the sink refuses.
    pub fn apply(&self, sink: &mut impl BufferSink) -> Result<(), String> {
        self.writes
            .iter()
            .try_for_each(|write| sink.write(write.id, 0, &write.bytes))?;
        self.pixels
            .iter()
            .try_for_each(|answer| sink.write_pixels(answer.request, &answer.rgba))
    }
}

/// What one submission of a flush left to resolve: the probe's second read, the copy a pixel read
/// wanted (or why it could not be made) and the submission they wait for.
#[derive(Default)]
struct Submitted {
    before: Option<BeforeDispatch>,
    read: Option<Result<Copied, String>>,
    submission: Option<wgpu::SubmissionIndex>,
}

impl Submitted {
    fn resolve(
        self,
        device: &wgpu::Device,
        report: &mut Report,
        pixels: &mut Vec<PixelAnswer>,
        dispatched: bool,
        probe: Option<Probe>,
    ) {
        let Self {
            before,
            read,
            submission,
        } = self;
        match (read, &submission) {
            (Some(Ok(copy)), Some(submission)) => match copy.resolve(submission) {
                Ok(answer) => {
                    report.pixel_reads += 1;
                    pixels.push(answer);
                }
                Err(reason) => refuse_read(report, reason),
            },
            (Some(Err(reason)), _) => refuse_read(report, &reason),
            _ => {}
        }
        if dispatched && let Some(trace) = report.traces.last_mut() {
            "ran".clone_into(&mut trace.outcome);
            *report
                .census
                .entry("dispatches ran".to_owned())
                .or_default() += 1;
            if let (Some(probe), Some(before), Some(submission)) = (probe, before, &submission) {
                probe.resolve(before, submission, device, trace);
            }
        }
    }
}

/// Whether `record` only sets the state the records after it draw with.
fn is_state(record: &Record) -> bool {
    matches!(record, Record::Globals(_))
}

/// `segment` with the last globals record before it in front, unless it starts with one: a
/// segment is built alone, and the globals in force at its start are set by an earlier segment.
fn with_globals(before: &[Record], segment: &[Record]) -> Vec<Record> {
    let mut records = Vec::with_capacity(segment.len() + 1);
    if !segment.first().is_some_and(is_state) {
        records.extend(before.iter().rev().find(|record| is_state(record)).cloned());
    }
    records.extend_from_slice(segment);
    records
}

/// Counts a pixel read left unanswered.
fn refuse_read(report: &mut Report, reason: &str) {
    *report
        .pixel_reads_refused
        .entry(reason.to_owned())
        .or_default() += 1;
}

/// The player's side of a device: its runtime, arena, state and adapters.
pub struct OffscreenHost {
    runtime: ProgramRuntime,
    arena: ProgramArena,
    state: OffscreenState,
    adapter: OffscreenAdapter,
    compute: ComputeAdapter,
    traces: Vec<WorkTrace>,
    /// The compute buffers the last frame's materials bind, kept until the next frame because
    /// the scene submits what `frame` built after it returns.
    frame_buffers: GpuBuffers,
    /// Whether submitted work has drawn into the screen's colour since it was last read back.
    screen_written: bool,
    /// Whether the last `frame`'s operations draw into the screen; counted once submitted.
    frame_writes_screen: bool,
    /// The allocation last written by a submitted flush. A handle does not keep it alive.
    screen_flushed: Option<TextureHandle>,
    /// The most draws a build plans before the flush submits them and plans the next (see
    /// [`set_draw_budget`](Self::set_draw_budget)).
    draw_budget: usize,
}

impl OffscreenHost {
    /// Bytes of GPU storage the runtime may hold.
    #[must_use]
    pub fn storage_budget(&self) -> u64 {
        self.runtime.budget().0
    }

    /// The most bytes of GPU storage the runtime has held at once.
    #[must_use]
    pub fn peak_bytes(&self) -> u64 {
        self.runtime.peak_bytes()
    }

    /// Bytes of GPU storage the runtime holds now: the render textures and uploads it keeps, and
    /// storage released but possibly still in use.
    #[must_use]
    pub fn live_bytes(&self) -> u64 {
        self.runtime.live_bytes()
    }

    /// The authority the runtime records and submits through.
    #[cfg(test)]
    pub(crate) fn authority(&self) -> &worldrunner_render_gpu::submission::SubmissionAuthority {
        self.runtime.authority()
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
            screen_written: false,
            frame_writes_screen: false,
            screen_flushed: None,
            frame_buffers: GpuBuffers::default(),
            draw_budget: DRAW_BUDGET,
        }
    }

    /// Sets the most draws a flush plans in one build. A run of records that would plan more is
    /// built, submitted and dropped in parts, in record order.
    pub fn set_draw_budget(&mut self, draws: usize) {
        self.draw_budget = draws;
    }

    /// Gives back the buffers the last frame's materials bound.
    fn release_frame_buffers(&mut self) {
        for handle in std::mem::take(&mut self.frame_buffers).handles() {
            let _ = self.runtime.release_buffer(handle);
        }
    }

    /// Sets the size of the screen a `CameraTarget` with no camera texture draws into, or takes
    /// the screen away with `None`. The next frame or flush that names it makes it at the new
    /// size, empty. Taking the screen away gives back its storage and forgets what was written
    /// to it, so nothing of it is read afterwards.
    pub fn set_screen(&mut self, size: Option<[u32; 2]>) {
        self.adapter.set_screen(size);
        if size.is_some() {
            return;
        }
        self.screen_written = false;
        self.frame_writes_screen = false;
        self.screen_flushed = None;
        let screen = ids::screen_color();
        if self
            .state
            .release_render_texture(&mut self.runtime, screen)
            .is_some()
        {
            self.settle();
        }
    }

    /// Whether work has drawn into the screen since the last
    /// [`take_screen_written`](Self::take_screen_written).
    #[must_use]
    pub fn screen_written(&self) -> bool {
        self.screen_written
    }

    /// Whether work has drawn into the screen since the last call, which starts the count again.
    pub fn take_screen_written(&mut self) -> bool {
        std::mem::take(&mut self.screen_written)
    }

    /// Tells the host that the operations the last [`frame`](Self::frame) gave were submitted, or
    /// handed to the viewer, which runs them before anything reads the screen, so the screen
    /// holds what they drew.
    pub fn frame_submitted(&mut self) {
        self.screen_written |= std::mem::take(&mut self.frame_writes_screen);
    }

    /// Reads the screen back: the image the screen holds now, which `frame` numbers. Tests read
    /// what draws left in the screen with it; a run shows the screen from its texture. The copy is
    /// submitted and waited for on its own, after every submission before it. A refusal is the
    /// reason, for the caller to count (see the `screen` module), and leaves the screen marked as
    /// written.
    ///
    /// # Errors
    /// Fails with the reason when the screen has no storage, the copy cannot be made or read, or
    /// the screen is not four 8-bit channels.
    #[cfg(test)]
    pub fn read_screen(&mut self, frame: u64) -> Result<ScreenImage, String> {
        // The frame that stages the copy is not begun yet, so storage can still be released.
        self.make_room_to_read(ids::screen_color());
        let handle = self
            .state
            .texture_handle(ids::screen_color())
            .ok_or(pixels::REFUSED_NO_TEXTURE)?;
        let authority = self.runtime.authority().clone();
        let device = self.runtime.device().clone();
        let mut encoder = authority
            .begin(&device)
            .map_err(|error| ProgramError::from(error).to_string())?;
        let copy = self
            .runtime
            .read_texture(&mut encoder, handle)
            .map_err(|error| pixels::no_copy(&error))?;
        let submission = encoder
            .finish()
            .submit()
            .map_err(|error| ProgramError::from(error).to_string())?;
        let image = copy
            .resolve(submission)
            .map_err(|_| pixels::REFUSED_NOT_READ)?;
        let screen = ScreenImage::from_gpu_rows(&image, frame)?;
        self.take_screen_written();
        Ok(screen)
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
            camera_renders: report.camera_draws.len() + report.list_draws.len(),
            drawn: report.camera_draws.iter().map(|c| c.drawn).sum::<usize>()
                + report.list_draws.iter().map(|l| l.drawn).sum::<usize>(),
            left_out: report
                .camera_draws
                .iter()
                .map(CameraDraws::left_out)
                .sum::<usize>()
                + report
                    .list_draws
                    .iter()
                    .map(ListDraws::left_out)
                    .sum::<usize>(),
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

    /// Sets the most bytes all of a frame's constants may take together.
    pub fn set_constants_total_limit(&mut self, bytes: u64) {
        self.state.set_constants_total_limit(bytes);
    }

    /// The runtime, for a caller that draws with the same device.
    pub fn runtime_mut(&mut self) -> &mut ProgramRuntime {
        &mut self.runtime
    }

    /// Adapts and builds the records of `snapshot` for the scene's next frame. A frame with no
    /// records gives `None`. `settling` is the way giving storage back for the frame's room gets
    /// the GPU to finish with it. A frame the viewer submits later uses [`Settling::Poll`], which
    /// collects what the GPU has finished and never waits for the viewer's submission.
    ///
    /// # Errors
    /// Fails with the error of the records that are left when they still do not build together.
    pub(crate) fn frame(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
        settling: Settling,
        reads: &[crate::TargetRead],
    ) -> Result<(Option<OffscreenOperations>, Report), OffscreenError> {
        let live_before = self.runtime.live_bytes();
        self.release_frame_buffers();
        let started = Instant::now();
        self.frame_writes_screen = false;
        let uploaded = self
            .frame_buffers
            .upload_material_buffers(&mut self.runtime, snapshot);
        let mut adapted = self.adapter.adapt(
            snapshot,
            assets,
            (&mut self.state, &mut self.runtime),
            (false, &uploaded.handles),
            settling,
        );
        adapted
            .report
            .count_many("materialbuffer:BufferUpload", uploaded.refused);
        adapted.report.phases.adapt = started.elapsed();
        taps::arrange(&mut adapted, reads);
        if adapted.frame.records.is_empty() {
            self.release_frame_buffers();
            self.note(("frame", live_before), &adapted.report, None);
            return Ok((None, adapted.report));
        }
        let reclaimed =
            self.adapter
                .make_room(&mut self.state, &mut self.runtime, &adapted.frame, settling);
        adapted.report.reclaimed.add(reclaimed);
        let built = build(
            &mut self.state,
            &mut self.runtime,
            &self.arena,
            &mut adapted,
        );
        let error = built.as_ref().err().map(ToString::to_string);
        self.note(("frame", live_before), &adapted.report, error);
        let operations = built?;
        self.frame_writes_screen = writes_screen(&adapted.frame.records);
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
        let live_before = self.runtime.live_bytes();
        let started = Instant::now();
        // The render textures a kernel writes need storage binding usage when they are made.
        let storage = self.compute.storage_textures(&self.runtime, snapshot);
        self.state.bind_as_storage(storage);
        // The materials' buffers go up before the frame is adapted, and a dispatch that binds one
        // of them finds it held.
        let mut buffers = GpuBuffers::default();
        let uploaded = buffers.upload_material_buffers(&mut self.runtime, snapshot);
        let mut adapted = self.adapter.adapt(
            snapshot,
            assets,
            (&mut self.state, &mut self.runtime),
            (true, &uploaded.handles),
            Settling::Wait,
        );
        adapted
            .report
            .count_many("materialbuffer:BufferUpload", uploaded.refused);
        let reclaimed = self.adapter.make_room(
            &mut self.state,
            &mut self.runtime,
            &adapted.frame,
            Settling::Wait,
        );
        adapted.report.reclaimed.add(reclaimed);
        adapted.report.phases.adapt = started.elapsed();
        let result = self.run(snapshot, &mut adapted, &mut buffers);
        let error = result.as_ref().err().map(ToString::to_string);
        // The buffers can go once the GPU has finished with them, and the next flush then
        // allocates on top of what is live, not of what this one released.
        for handle in buffers.handles() {
            let _ = self.runtime.release_buffer(handle);
        }
        self.settle();
        self.note(("flush", live_before), &adapted.report, error);
        result.map(|produced| FlushOutcome {
            writes: produced.writes,
            pixels: produced.pixels,
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

    /// Frees the storage released meanwhile that the GPU has finished with, without waiting for
    /// anything. A window's frames are submitted by the viewer, so the host cannot wait for them;
    /// the runtime frees only what a completed submission was the last to use.
    pub fn collect_completed(&mut self) {
        // A poll that fails leaves the completed count where it was, which frees less.
        let _ = self.runtime.device().poll(wgpu::PollType::Poll);
        self.runtime.collect();
    }

    fn run(
        &mut self,
        snapshot: &GraphicsSnapshot,
        adapted: &mut Adapted,
        buffers: &mut GpuBuffers,
    ) -> Result<Produced, FlushError> {
        let placements = std::mem::take(&mut adapted.placements);
        let records = std::mem::take(&mut adapted.frame.records);
        let mut built = 0;
        let mut from = 0;
        let mut pixels = Vec::new();
        for (index, until) in placements
            .iter()
            .map(|placed| placed.after)
            .chain(std::iter::once(records.len()))
            .enumerate()
        {
            // The parts before the last are built and submitted now, so their planned draws are
            // dropped before the next part plans its own.
            let mut parts = parts::split(&adapted.frame, &records, from..until, self.draw_budget);
            let last = parts.pop().unwrap_or(from..until);
            built += self.submit_parts(&records, &parts, adapted)?;
            from = until;
            // The first build also creates the render textures the dispatches bind.
            let skippable = index > 0 || placements.is_empty();
            let (mut operations, kept, wrote_screen) =
                self.build_last_part(&records, last, skippable, adapted)?;
            built += kept;
            let work = placements.get(index).map(|placed| placed.work);
            let prepared =
                work.and_then(|work| self.prepare(snapshot, work, buffers, &mut adapted.report));
            let probe = prepared.as_ref().and_then(|_| {
                let trace = adapted.report.traces.last()?;
                Some(Probe::plan(trace, &self.state, operations.as_mut()))
            });
            let wanted = work.and_then(|work| Wanted::of(&snapshot.work[work]));
            if let Some(wanted) = &wanted {
                self.make_room_for_read(wanted, &mut adapted.report);
            }
            let submitted = Instant::now();
            let result = self.submit(operations, prepared.as_ref(), probe.as_ref(), wanted)?;
            adapted.report.phases.submit += submitted.elapsed();
            self.note_screen_written(wrote_screen);
            result.resolve(
                self.runtime.device(),
                &mut adapted.report,
                &mut pixels,
                prepared.is_some(),
                probe,
            );
        }
        adapted.report.built = built;
        let writes = self.read_back(buffers, &mut adapted.report.traces)?;
        Ok(Produced { writes, pixels })
    }

    /// Builds the last part of a run, which the run's dispatch or read follows in its own
    /// submission. A part of state records alone builds nothing when `skippable`. Returns the
    /// operations, the records kept and whether they write the screen.
    fn build_last_part(
        &mut self,
        records: &[Record],
        part: Range<usize>,
        skippable: bool,
        adapted: &mut Adapted,
    ) -> Result<(Option<OffscreenOperations>, usize, bool), FlushError> {
        let segment = with_globals(&records[..part.start], &records[part]);
        if skippable && segment.iter().all(is_state) {
            return Ok((None, 0, false));
        }
        let (operations, kept, wrote_screen) =
            self.build_segment(&adapted.frame, segment, &mut adapted.report)?;
        Ok((Some(operations), kept, wrote_screen))
    }

    /// Builds and submits each of `parts` of `records` on its own, and returns how many of their
    /// records were kept. A later part that fails does not undo what an earlier one drew.
    fn submit_parts(
        &mut self,
        records: &[Record],
        parts: &[Range<usize>],
        adapted: &mut Adapted,
    ) -> Result<usize, FlushError> {
        let mut built = 0;
        for part in parts {
            let segment = with_globals(&records[..part.start], &records[part.clone()]);
            let (operations, kept, wrote_screen) =
                self.build_segment(&adapted.frame, segment, &mut adapted.report)?;
            built += kept;
            let submitted = Instant::now();
            self.submit(Some(operations), None, None, None)?;
            adapted.report.phases.submit += submitted.elapsed();
            self.note_screen_written(wrote_screen);
        }
        Ok(built)
    }

    /// Notes that a submitted part drew into the screen's colour.
    fn note_screen_written(&mut self, wrote_screen: bool) {
        // A later segment that fails does not undo what this one drew.
        self.screen_written |= wrote_screen;
        if wrote_screen {
            self.note_screen_flushed();
        }
    }

    fn build_segment(
        &mut self,
        frame: &OffscreenFrame,
        segment: Vec<Record>,
        report: &mut Report,
    ) -> Result<(OffscreenOperations, usize, bool), FlushError> {
        let mut part = Adapted {
            frame: OffscreenFrame {
                records: segment,
                ..frame.clone()
            },
            report: Report::default(),
            placements: Vec::new(),
        };
        let built = build(&mut self.state, &mut self.runtime, &self.arena, &mut part);
        // A refused segment still tells how many refusals and probes it cost and how long.
        let kept = part
            .frame
            .records
            .iter()
            .filter(|record| {
                !is_test_render(record, &frame.probe.test_targets) && !is_state(record)
            })
            .count();
        let wrote_screen = writes_screen(&part.frame.records);
        report.absorb_failures(part.report);
        Ok((built?, kept, wrote_screen))
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
            for list in report
                .list_draws
                .iter()
                .filter(|list| Some(list.target) == texture.texture)
            {
                texture.camera_renders += 1;
                texture.camera_draws += list.drawn;
                texture.camera_unmatched += list.unmatched;
                texture.camera_missing_texture += list.missing_texture;
                texture.camera_missing_channel += list.missing_channel;
            }
            for (camera, pass) in report
                .camera_draws
                .iter()
                .zip(&report.camera_passes)
                .filter(|(camera, _)| Some(camera.target) == texture.texture)
            {
                texture.camera_renders += 1;
                texture.camera_draws += camera.drawn;
                texture.camera_unmatched += camera.unmatched;
                texture.camera_missing_texture += camera.missing_texture;
                texture.camera_missing_channel += camera.missing_channel;
                texture.camera_notes.push(format!(
                    "camera {:?} drew {} into color {:?} depth {:?}, states {:?}, without a \
                     value {:?}",
                    camera.camera,
                    camera.drawn,
                    pass.color,
                    pass.depth,
                    pass.states,
                    pass.without_value
                ));
                texture.camera_notes.extend(pass_notes::notes(pass));
                for source in pass_notes::sampled(pass) {
                    if !texture
                        .sampled
                        .iter()
                        .any(|held| held.texture == source.texture)
                    {
                        texture.sampled.push(source);
                    }
                }
            }
        }
        for pairing in &trace.pairings {
            *report.census.entry(pairing.clone()).or_default() += 1;
        }
        let outcome = match prepared {
            Ok(prepared) => Some(prepared),
            Err(name) => {
                let cause = format!("dispatch:{name}");
                report.count(&cause);
                if !trace.reason.is_empty() {
                    report
                        .examples
                        .entry(cause)
                        .or_insert_with(|| trace.reason.clone());
                }
                name.clone_into(&mut trace.outcome);
                None
            }
        };
        report.traces.push(trace);
        outcome
    }

    /// Gives back what the snapshot does not name when the budget has no room for the staging a
    /// read of its texture takes. The frame that stages the copy is not begun yet, so storage can
    /// still be released.
    fn make_room_for_read(&mut self, wanted: &Wanted, report: &mut Report) {
        let reclaimed = self.make_room_to_read(ids::render_texture(wanted.texture));
        report.reclaimed.add(reclaimed);
    }

    /// Gives back what the snapshot does not name when the budget has no room for the staging a
    /// read of `texture` takes, and tells what it gave back.
    fn make_room_to_read(
        &mut self,
        texture: worldrunner_render_program::ids::TextureId,
    ) -> Reclaimed {
        let Some(bytes) = self.state.read_bytes(texture) else {
            return Reclaimed::default();
        };
        // A read waits for its own submission, which follows every other, so it can wait now.
        self.adapter
            .make_room_for(&mut self.state, &mut self.runtime, bytes, Settling::Wait)
    }

    /// Records the operations, then the dispatch, then the copy a pixel read wants into one frame
    /// and submits it. The probe's second read is recorded between the operations and the
    /// dispatch.
    fn submit(
        &mut self,
        operations: Option<OffscreenOperations>,
        dispatch: Option<&Prepared>,
        probe: Option<&Probe>,
        wanted: Option<Wanted>,
    ) -> Result<Submitted, FlushError> {
        if operations.is_none() && dispatch.is_none() && wanted.is_none() {
            return Ok(Submitted::default());
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
        let before = probe
            .filter(|_| dispatch.is_some())
            .map(|probe| probe.record(&self.runtime, &mut frame));
        if let Some(dispatch) = dispatch {
            self.runtime.dispatch(&mut frame, &dispatch.request())?;
        }
        let read = wanted.map(|wanted| self.copy_for(wanted, &mut frame));
        let submission = frame.finish().submit().map_err(ProgramError::from)?;
        Ok(Submitted {
            before,
            read,
            submission: Some(submission),
        })
    }

    /// Records the copy of the render texture a pixel read wants, after everything in `frame`.
    fn copy_for(&self, wanted: Wanted, frame: &mut wgpu::CommandEncoder) -> Result<Copied, String> {
        let handle = self
            .state
            .texture_handle(ids::render_texture(wanted.texture))
            .ok_or(pixels::REFUSED_NO_TEXTURE)?;
        let copy = self
            .runtime
            .read_texture(frame, handle)
            .map_err(|error| pixels::no_copy(&error))?;
        Ok(Copied::new(wanted, copy))
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
