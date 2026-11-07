//! The offscreen work of a built player in a host loop with a device.
//!
//! [`PlayerGpu`] makes the device's [`OffscreenHost`] once: the program runtime, its arena and the
//! adapters that load the player's shaders, meshes and textures. It does the player's GPU work
//! two ways, both through that one host:
//!
//! * Each frame, [`PlayerGpu::frame`] takes the world's graphics snapshot. A frame with no
//!   offscreen work changes nothing: the host does not touch the device. Otherwise the records
//!   are adapted, built and run in one submission.
//! * When a script reads a compute buffer back, the managed engine calls the flush handler
//!   [`PlayerGpu::install`] registers. It runs the work recorded so far, dispatches included,
//!   waits for the submission, and writes the changed buffers back before the read returns. It
//!   reads meshes, textures and samplers through [`GpuFlush`], so one the flush names for the
//!   first time is there for that flush.
//!

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::{DisunityHost, GpuFlush};
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;

use crate::PlayError;
use crate::offscreen::{
    ComputeAdapter, DispatchTrace, OffscreenAdapter, OffscreenHost, PlayerKernels, Report,
    Settings, WorkTrace,
};

/// Bytes of GPU storage the program runtime may hold.
const RUNTIME_BUDGET: u64 = 512 << 20;

/// The most dispatch traces a report keeps.
const MAX_TRACES: usize = 32;

/// The most frame and flush traces a report keeps.
const MAX_WORK_TRACES: usize = 64;

/// The textures the world's globals held when one piece of work was mapped, as `name: kind`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalListing {
    /// The frame the work belongs to, counting from zero: a flush the managed engine asked for
    /// during a frame's script update, or that frame's own snapshot.
    pub frame: u64,
    /// `flush` or `frame`.
    pub source: &'static str,
    pub textures: Vec<String>,
}

impl GlobalListing {
    /// The listing as a report line.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "globals at a {} of frame {}: {}",
            self.source,
            self.frame,
            self.textures.join(", ")
        )
    }
}

/// What the offscreen work of a run did.
#[derive(Debug, Default)]
pub struct GpuWorkReport {
    /// Frames the player has run the frame path for, those without work included.
    pub frames: u64,
    /// The textures the globals held at each piece of work that listed any, in order.
    pub global_listings: Vec<GlobalListing>,
    /// Frames whose snapshot held work.
    pub frames_with_work: u64,
    /// Flushes the managed engine asked for.
    pub flushes: u64,
    /// Records the world listed, the frame records built from them and the dispatches among them.
    pub records: usize,
    pub built: usize,
    pub dispatches: usize,
    /// Records left out by reason, renderers left out by reason, and what could not be loaded,
    /// uploaded or built by the kind and the name of the error.
    pub skipped: BTreeMap<String, usize>,
    pub renderers_skipped: BTreeMap<String, usize>,
    pub failures: BTreeMap<String, usize>,
    /// The failures by the chain of names inside the error, and the first message of each.
    pub causes: BTreeMap<String, usize>,
    pub examples: BTreeMap<String, String>,
    /// Compute buffers written back and the bytes of them that are not zero.
    pub buffers_written: usize,
    pub nonzero_values: usize,
    /// The most bytes of GPU storage the runtime held after a flush.
    pub peak_live_bytes: u64,
    /// The records of frames and flushes the GPU refused whole. Their other counts are not in the
    /// totals above, which a refused piece of work never reaches.
    pub refused_records: usize,
    pub refused_works: usize,
    /// Why a frame's work or a flush failed, by message.
    pub errors: BTreeMap<String, usize>,
    /// How the first dispatches were bound and what came of them.
    pub traces: Vec<DispatchTrace>,
    /// Camera renders, the draws they recorded and the draws left out for want of a pass.
    pub camera_renders: usize,
    pub camera_draws: usize,
    pub camera_unmatched: usize,
    /// The draws left out for want of a pass and the partial texture reads, by description of
    /// the passes concerned.
    pub unmatched_passes: BTreeMap<String, usize>,
    pub partial_reads: BTreeMap<String, usize>,
    /// The draws left out because no layer names a texture the pass samples, by the property.
    pub missing_textures: BTreeMap<String, usize>,
    /// The draws left out because the mesh lacks a vertex channel the pass reads.
    pub missing_channels: BTreeMap<String, usize>,
    /// The draws that read a constant for a vertex channel their mesh lacks, which are recorded.
    pub defaulted_channels: BTreeMap<String, usize>,
    /// The texture properties that named no texture the frame can read, by `name: reason`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// The script commands left out, and the textures the globals held, by description.
    pub unsupported_work: BTreeMap<String, usize>,
    pub global_textures: BTreeMap<String, usize>,
    /// Time spent in the frame path and in flushes.
    pub frame_time: Duration,
    pub flush_time: Duration,
    /// Each frame's and flush's own steps, in order, up to `MAX_WORK_TRACES`.
    pub work: Vec<WorkTrace>,
}

impl GpuWorkReport {
    pub(crate) fn keep_traces(&mut self, traces: Vec<WorkTrace>) {
        for trace in traces.iter().filter(|trace| trace.error.is_some()) {
            self.refused_records += trace.records;
            self.refused_works += 1;
        }
        let room = MAX_WORK_TRACES.saturating_sub(self.work.len());
        self.work.extend(traces.into_iter().take(room));
    }

    fn add(&mut self, report: &Report, source: &'static str) {
        self.records += report.records;
        self.built += report.built;
        self.dispatches += report.dispatches;
        for (skip, count) in &report.skipped {
            *self.skipped.entry(format!("{skip:?}")).or_default() += count;
        }
        for (skip, count) in &report.renderers_skipped {
            *self
                .renderers_skipped
                .entry(format!("{skip:?}"))
                .or_default() += count;
        }
        for (name, count) in &report.failures {
            *self.failures.entry(name.clone()).or_default() += count;
        }
        for (cause, count) in &report.causes {
            *self.causes.entry(cause.clone()).or_default() += count;
        }
        for (cause, example) in &report.examples {
            self.examples
                .entry(cause.clone())
                .or_insert_with(|| example.clone());
        }
        let room = MAX_TRACES.saturating_sub(self.traces.len());
        self.traces.extend(report.traces.iter().take(room).cloned());
        self.camera_renders += report.camera_draws.len();
        self.camera_draws += report.camera_draws.iter().map(|c| c.drawn).sum::<usize>();
        self.camera_unmatched += report
            .camera_draws
            .iter()
            .map(|c| c.unmatched)
            .sum::<usize>();
        for (name, count) in &report.missing_textures {
            *self.missing_textures.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.missing_channels {
            *self.missing_channels.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.defaulted_channels {
            *self.defaulted_channels.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.unsupported_work {
            *self.unsupported_work.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.global_textures {
            *self.global_textures.entry(name.clone()).or_default() += count;
        }
        if !report.global_textures.is_empty() {
            self.global_listings.push(GlobalListing {
                frame: self.frames,
                source,
                textures: report.global_textures.keys().cloned().collect(),
            });
        }
        for (name, count) in &report.unresolved_textures {
            *self.unresolved_textures.entry(name.clone()).or_default() += count;
        }
        for entry in &report.unmatched_passes {
            let name = format!(
                "{} selected {:?}, the shader's {} passes have {:?}",
                entry.key, entry.selected, entry.passes, entry.tags
            );
            *self.unmatched_passes.entry(name).or_default() += entry.draws;
        }
        for entry in &report.partial_reads {
            let name = format!(
                "{} {} of {} levels, read by the pass with {} {:?}",
                entry.texture, entry.written, entry.total, entry.key, entry.tag
            );
            *self.partial_reads.entry(name).or_default() += entry.reads;
        }
    }

    /// Writes the counts, one line each, to standard error.
    pub fn print(&self) {
        eprintln!(
            "[gpu] frames with work {}, flushes {}, records {}, built {}, dispatches {}",
            self.frames_with_work, self.flushes, self.records, self.built, self.dispatches
        );
        for (title, counts) in [
            ("skipped records", &self.skipped),
            ("skipped renderers", &self.renderers_skipped),
            ("failures", &self.failures),
            ("causes", &self.causes),
            ("errors", &self.errors),
        ] {
            for (name, count) in counts {
                eprintln!("[gpu] {title}: {name} {count}");
            }
        }
        for (cause, example) in &self.examples {
            eprintln!("[gpu] example of {cause}: {example}");
        }
        eprintln!(
            "[gpu] camera renders {}, draws recorded {}, draws left out for want of a pass {}",
            self.camera_renders, self.camera_draws, self.camera_unmatched
        );
        for (title, counts) in [
            ("draws without a pass", &self.unmatched_passes),
            ("partial texture reads", &self.partial_reads),
            ("draws without a texture", &self.missing_textures),
            ("draws without a mesh channel", &self.missing_channels),
            ("draw:DefaultedChannel", &self.defaulted_channels),
            ("script commands left out", &self.unsupported_work),
            ("global textures at a flush", &self.global_textures),
            (
                "texture properties without a texture",
                &self.unresolved_textures,
            ),
        ] {
            for (name, count) in counts {
                eprintln!("[gpu] {title}: {name}: {count}");
            }
        }
        for trace in &self.traces {
            for line in trace.lines() {
                eprintln!("[gpu] {line}");
            }
        }
        for listing in &self.global_listings {
            eprintln!("[gpu] {}", listing.line());
        }
        eprintln!(
            "[gpu] compute buffers written {}, non-zero values {}",
            self.buffers_written, self.nonzero_values
        );
        eprintln!(
            "[gpu] refused whole: {} frames or flushes with {} records",
            self.refused_works, self.refused_records
        );
        eprintln!(
            "[gpu] most storage held after a flush {} bytes",
            self.peak_live_bytes
        );
        eprintln!(
            "[gpu] time in the frame path {:?}, in flushes {:?}",
            self.frame_time, self.flush_time
        );
        for (index, work) in self.work.iter().enumerate() {
            eprintln!("[gpu] {}", work.line(index));
        }
    }
}

/// The device the headless player asks for: the default limits and block compression where offered, except that one buffer may hold
/// the whole storage budget, since the program arena is reserved as one buffer of that size.
fn device_requirements() -> DeviceRequirements {
    DeviceRequirements {
        // The player stores some textures block-compressed; a device without the feature leaves
        // them out by name.
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        required_limits: wgpu::Limits {
            max_buffer_size: RUNTIME_BUDGET,
            ..wgpu::Limits::default()
        },
        ..DeviceRequirements::default()
    }
}

/// A device and the player's offscreen host on it.
pub struct PlayerGpu {
    offscreen: Rc<RefCell<OffscreenHost>>,
    report: Rc<RefCell<GpuWorkReport>>,
    authority: SubmissionAuthority,
    device: wgpu::Device,
}

impl PlayerGpu {
    /// Bytes of GPU storage the runtime may hold.
    #[must_use]
    pub fn storage_budget(&self) -> u64 {
        self.offscreen.borrow().storage_budget()
    }

    /// A device without a window and the host on it, with shaders loaded from the player in
    /// `data`.
    ///
    /// # Errors
    /// Fails with the reason when the machine has no adapter, the device cannot be had or the
    /// program runtime cannot reserve its arena. The run goes on without GPU work then.
    pub fn headless(data: &Path) -> Result<Self, String> {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|error| format!("no adapter: {error}"))?;
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &device_requirements()))
                .map_err(|error| format!("no device: {error}"))?;
        let authority = SubmissionAuthority::new(queue);
        let budget = AllocationBudget(RUNTIME_BUDGET);
        let runtime = ProgramRuntime::new(device.clone(), authority.clone(), budget);
        let limits = device.limits();
        let mut renderer = Configuration::new(
            &authority,
            (),
            AllocationSet::default(),
            &limits,
            AllocationBudget(RUNTIME_BUDGET * 4),
        )
        .map_err(|error| format!("no renderer configuration: {error}"))?;
        let reservation = renderer
            .reserve::<()>(&limits, ProgramRuntime::arena_allocations(budget.0))
            .map_err(|error| format!("no arena reservation: {error:?}"))?;
        let arena = runtime
            .open_arena(reservation)
            .map_err(|error| format!("no arena: {error}"))?;
        let settings = Settings::cameras();
        let offscreen = OffscreenHost::new(
            runtime,
            arena,
            OffscreenAdapter::new(data, settings),
            ComputeAdapter::new(PlayerKernels::new(data)),
        );
        Ok(Self {
            offscreen: Rc::new(RefCell::new(offscreen)),
            report: Rc::new(RefCell::new(GpuWorkReport::default())),
            authority,
            device,
        })
    }

    /// Registers the flush handler with `host`, for the world it runs now and later ones.
    ///
    /// # Errors
    /// Fails when the engine image has no flush registration.
    pub fn install(&self, host: &mut DisunityHost) -> Result<(), PlayError> {
        let offscreen = Rc::clone(&self.offscreen);
        let report = Rc::clone(&self.report);
        host.set_gpu_flush(Box::new(move |flush: &mut GpuFlush<'_>| {
            let started = Instant::now();
            let snapshot = flush.snapshot().map_err(|error| error.to_string())?;
            let outcome = offscreen
                .borrow_mut()
                .flush(&snapshot, &*flush)
                .map_err(|error| error.to_string());
            let mut report = report.borrow_mut();
            report.keep_traces(offscreen.borrow_mut().take_traces());
            report.flushes += 1;
            report.flush_time += started.elapsed();
            report.peak_live_bytes = report.peak_live_bytes.max(offscreen.borrow().live_bytes());
            let outcome = outcome.inspect_err(|message| {
                *report.errors.entry(message.clone()).or_default() += 1;
            })?;
            report.add(&outcome.report, "flush");
            report.buffers_written += outcome.writes.len();
            report.nonzero_values += outcome
                .writes
                .iter()
                .flat_map(|write| write.bytes.chunks_exact(4))
                .filter(|word| word.iter().any(|byte| *byte != 0))
                .count();
            outcome.apply(flush)
        }))?;
        Ok(())
    }

    /// Takes the frame's snapshot and runs its offscreen work. A frame with none returns at once
    /// and leaves the device alone.
    ///
    /// # Errors
    /// Fails when the host cannot give the snapshot. Work the GPU refuses is counted in the
    /// report, not an error: the player goes on without it.
    pub fn frame(&self, host: &mut DisunityHost) -> Result<(), PlayError> {
        let snapshot = host.graphics_snapshot()?;
        if snapshot.work.is_empty() {
            self.report.borrow_mut().frames += 1;
            return Ok(());
        }
        let started = Instant::now();
        let mut report = self.report.borrow_mut();
        report.frames_with_work += 1;
        let built = self.offscreen.borrow_mut().frame(&snapshot, &*host);
        report.keep_traces(self.offscreen.borrow_mut().take_traces());
        match built {
            Ok((operations, frame_report)) => {
                report.add(&frame_report, "frame");
                if let Some(operations) = operations {
                    let submitted = Instant::now();
                    let ran = self.run(operations);
                    self.offscreen.borrow_mut().settle();
                    if let Some(work) = report.work.last_mut() {
                        work.phases.submit += submitted.elapsed();
                    }
                    if let Err(message) = ran {
                        *report.errors.entry(message).or_default() += 1;
                    }
                }
            }
            Err(error) => {
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
        }
        report.frame_time += started.elapsed();
        report.frames += 1;
        Ok(())
    }

    /// Records the operations into a frame of their own, submits it and waits for it.
    fn run(
        &self,
        operations: worldrunner_render_program::unity::offscreen::OffscreenOperations,
    ) -> Result<(), String> {
        let mut frame = self
            .authority
            .begin(&self.device)
            .map_err(|error| error.to_string())?;
        let admitted = operations
            .admit(&frame)
            .map_err(|error| error.to_string())?;
        admitted
            .encode(None, &mut frame)
            .map_err(|error| error.to_string())?;
        let submission = frame.finish().submit().map_err(|error| error.to_string())?;
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// What the run's offscreen work did so far.
    #[must_use]
    pub fn report(&self) -> std::cell::Ref<'_, GpuWorkReport> {
        self.report.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(records: usize, error: Option<&str>) -> WorkTrace {
        WorkTrace {
            records,
            error: error.map(str::to_owned),
            ..WorkTrace::default()
        }
    }

    fn listing(names: &[&str]) -> Report {
        let mut report = Report::default();
        for name in names {
            report.global_textures.insert((*name).to_owned(), 1);
        }
        report
    }

    #[test]
    fn each_piece_of_work_lists_the_global_textures_it_saw_with_the_frame_it_belongs_to() {
        let mut report = GpuWorkReport::default();
        report.add(&listing(&["_Height: render"]), "flush");
        report.add(&Report::default(), "flush");
        report.frames = 3;
        report.add(&listing(&["_Height: render", "_Last: none"]), "flush");
        report.add(&listing(&["_Last: render"]), "frame");
        let found: Vec<_> = report
            .global_listings
            .iter()
            .map(GlobalListing::line)
            .collect();
        assert_eq!(
            found,
            [
                "globals at a flush of frame 0: _Height: render",
                "globals at a flush of frame 3: _Height: render, _Last: none",
                "globals at a frame of frame 3: _Last: render",
            ]
        );
    }

    #[test]
    fn work_the_gpu_refused_whole_is_counted_with_its_records_and_the_rest_is_not() {
        let mut report = GpuWorkReport::default();
        report.keep_traces(vec![
            trace(2513, Some("over its budget")),
            trace(27, None),
            trace(30, Some("a shader")),
        ]);
        assert_eq!((report.refused_works, report.refused_records), (2, 2543));
        assert_eq!(report.work.len(), 3, "every piece of work keeps its trace");
    }

    #[test]
    fn the_headless_device_can_hold_the_whole_storage_budget_in_one_arena_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let gpu = PlayerGpu::headless(directory.path()).unwrap();
        assert_eq!(gpu.storage_budget(), RUNTIME_BUDGET);
        assert!(
            gpu.storage_budget() > 256 << 20,
            "the Old World frames need more than the default buffer limit"
        );
    }
}
