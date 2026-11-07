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
    ComputeAdapter, DispatchTrace, OffscreenAdapter, OffscreenHost, PlayerKernels, Report, Settings,
};

/// Bytes of GPU storage the program runtime may hold.
const RUNTIME_BUDGET: u64 = 512 << 20;

/// The most dispatch traces a report keeps.
const MAX_TRACES: usize = 32;

/// What the offscreen work of a run did.
#[derive(Debug, Default)]
pub struct GpuWorkReport {
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
    /// The texture properties that named no texture the frame can read, by `name: reason`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// Time spent in the frame path and in flushes.
    pub frame_time: Duration,
    pub flush_time: Duration,
}

impl GpuWorkReport {
    fn add(&mut self, report: &Report) {
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
        eprintln!(
            "[gpu] compute buffers written {}, non-zero values {}",
            self.buffers_written, self.nonzero_values
        );
        eprintln!(
            "[gpu] most storage held after a flush {} bytes",
            self.peak_live_bytes
        );
        eprintln!(
            "[gpu] time in the frame path {:?}, in flushes {:?}",
            self.frame_time, self.flush_time
        );
    }
}

/// The device the headless player asks for: the default limits, except that one buffer may hold
/// the whole storage budget, since the program arena is reserved as one buffer of that size.
fn device_requirements() -> DeviceRequirements {
    DeviceRequirements {
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
            report.flushes += 1;
            report.flush_time += started.elapsed();
            report.peak_live_bytes = report.peak_live_bytes.max(offscreen.borrow().live_bytes());
            let outcome = outcome.inspect_err(|message| {
                *report.errors.entry(message.clone()).or_default() += 1;
            })?;
            report.add(&outcome.report);
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
            return Ok(());
        }
        let started = Instant::now();
        let mut report = self.report.borrow_mut();
        report.frames_with_work += 1;
        let built = self.offscreen.borrow_mut().frame(&snapshot, &*host);
        match built {
            Ok((operations, frame_report)) => {
                report.add(&frame_report);
                if let Some(operations) = operations
                    && let Err(message) = self.run(operations)
                {
                    *report.errors.entry(message).or_default() += 1;
                }
            }
            Err(error) => {
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
        }
        report.frame_time += started.elapsed();
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
