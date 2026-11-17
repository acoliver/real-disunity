//! The offscreen work of a built player in a host loop with a device.
//!
//! [`PlayerGpu`] makes the device's [`OffscreenHost`] once: the program runtime, its arena and the
//! adapters that load the player's shaders, meshes and textures. It does the player's GPU work
//! two ways, both through that one host:
//!
//! * Each frame, [`PlayerGpu::frame`] takes the world's graphics snapshot. A frame with no
//!   offscreen work changes nothing: the host does not touch the device. Otherwise the records
//!   are adapted, built and run in one submission, which the player waits for. A window does not
//!   wait: it makes the player on the viewer's device (`PlayerGpu::shared`) and hands each
//!   frame's built operations to the viewer (`PlayerGpu::frame_for_viewer`), which runs them in
//!   its own frame's submission.
//! * When a script reads a compute buffer back, the managed engine calls the flush handler
//!   [`PlayerGpu::install`] registers. It runs the work recorded so far, dispatches included,
//!   waits for the submission, and writes the changed buffers back before the read returns. It
//!   reads meshes, textures and samplers through [`GpuFlush`], so one the flush names for the
//!   first time is there for that flush.
//!

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::{DisunityHost, GpuFlush};
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::device::{headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;

mod frame;

use crate::PlayError;
use crate::gpu_device::{DeviceSource, player_device};
use crate::offscreen::{
    ComputeAdapter, DispatchTrace, OffscreenAdapter, OffscreenHost, PlayerKernels, Reclaimed,
    Report, ScreenImage, Settings, WorkTrace,
};

/// The most dispatch traces a report keeps.
const MAX_TRACES: usize = 32;

/// The most frame and flush traces a report keeps.
const MAX_WORK_TRACES: usize = 64;

/// The most textures the draw:MissingTexture line names.
const TOP_TEXTURES: usize = 5;

/// `title` and the `top` names with the largest counts, largest first and equal counts by name,
/// as `title: name count, name count`. A count of names past `top` ends the line as `and N more`.
/// There is no line for no counts.
fn top_counts_line(title: &str, counts: &BTreeMap<String, usize>, top: usize) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let mut ranked: Vec<_> = counts.iter().collect();
    // A stable sort keeps the names of equal counts in the map's order.
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    let named: Vec<String> = ranked
        .iter()
        .take(top)
        .map(|(name, count)| format!("{name} {count}"))
        .collect();
    let line = format!("{title}: {}", named.join(", "));
    Some(match ranked.len().saturating_sub(top) {
        0 => line,
        more => format!("{line}, and {more} more"),
    })
}

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
    /// The adapter and the buffer limits of the device the work ran on.
    pub device: String,
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
    /// Pixel reads of render textures answered, and left unanswered by the reason.
    pub pixel_reads: usize,
    pub pixel_reads_refused: BTreeMap<String, usize>,
    /// Images of the screen read back after work drew into it, and the readbacks refused by the
    /// reason.
    pub screen_reads: usize,
    pub screen_reads_refused: BTreeMap<String, usize>,
    /// The most bytes of GPU storage the runtime held after a flush.
    pub peak_live_bytes: u64,
    /// The most bytes the runtime held at any moment, not only after a flush.
    pub peak_runtime_bytes: u64,
    /// What the runtime held when the budget first refused an allocation, and the frame it was
    /// in.
    pub refusal_holdings: Vec<String>,
    pub refusal_frame: u64,
    /// The uploaded textures and meshes given back to make room.
    pub reclaimed: Reclaimed,
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
    /// The draws that read the grey default for a texture no layer set and the shader does not
    /// declare, which are recorded.
    pub undeclared_defaults: BTreeMap<String, usize>,
    /// The texture properties that named no texture the frame can read, by `name: reason`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// What the adapter mapped, by description (see `Report::census`).
    pub census: BTreeMap<String, usize>,
    /// What the culling records and renderer lists did, by description, and the draws the lists
    /// recorded and left out for want of a pass, a texture or a mesh channel.
    pub lists: BTreeMap<String, usize>,
    pub list_renders: usize,
    pub list_draws: usize,
    pub list_unmatched: usize,
    /// The textures the globals held, by description.
    /// The world's records the adapter ran or built, and those it left out as `kind: reason`.
    pub built_by_kind: BTreeMap<String, usize>,
    pub skipped_by_kind: BTreeMap<String, usize>,
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
        self.reclaimed.add(report.reclaimed);
        if self.refusal_holdings.is_empty() && !report.holdings.is_empty() {
            self.refusal_holdings.clone_from(&report.holdings);
            self.refusal_frame = self.frames;
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
        for (name, count) in &report.undeclared_defaults {
            *self.undeclared_defaults.entry(name.clone()).or_default() += count;
        }
        for (name, count) in report.lists.named() {
            *self.lists.entry(name).or_default() += count;
        }
        self.list_renders += report.list_draws.len();
        self.list_draws += report.list_draws.iter().map(|l| l.drawn).sum::<usize>();
        self.list_unmatched += report
            .list_draws
            .iter()
            .map(|l| l.unmatched + l.missing_texture + l.missing_channel)
            .sum::<usize>();
        for (name, count) in &report.built_by_kind {
            *self.built_by_kind.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.skipped_by_kind {
            *self.skipped_by_kind.entry(name.clone()).or_default() += count;
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
        for (name, count) in &report.census {
            *self.census.entry(name.clone()).or_default() += count;
        }
        self.pixel_reads += report.pixel_reads;
        for (reason, count) in &report.pixel_reads_refused {
            *self.pixel_reads_refused.entry(reason.clone()).or_default() += count;
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

    /// The line of the report about the screen's readbacks.
    fn screen_line(&self) -> String {
        let refused: usize = self.screen_reads_refused.values().sum();
        let reasons: Vec<String> = self
            .screen_reads_refused
            .iter()
            .map(|(reason, count)| format!("{reason}: {count}"))
            .collect();
        let detail = if reasons.is_empty() {
            String::new()
        } else {
            format!(" ({})", reasons.join("; "))
        };
        format!(
            "[gpu] screen images read back {}, refused {refused}{detail}",
            self.screen_reads
        )
    }

    /// Writes the counts, one line each, to standard error.
    pub fn print(&self) {
        eprintln!("[gpu] {}", self.device);
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
        eprintln!(
            "[gpu] draw lists {}, draws recorded {}, draws left out for want of a pass, texture or channel {}",
            self.list_renders, self.list_draws, self.list_unmatched
        );
        if let Some(line) = top_counts_line(
            "draw:MissingTexture by texture",
            &self.missing_textures,
            TOP_TEXTURES,
        ) {
            eprintln!("[gpu] {line}");
        }
        for (title, counts) in [
            ("draws without a pass", &self.unmatched_passes),
            ("partial texture reads", &self.partial_reads),
            ("draws without a mesh channel", &self.missing_channels),
            ("draw:DefaultedChannel", &self.defaulted_channels),
            ("draw:UndeclaredTextureGrey", &self.undeclared_defaults),
            ("renderer lists", &self.lists),
            ("records built, by kind", &self.built_by_kind),
            ("records skipped, by kind", &self.skipped_by_kind),
            ("global textures at a flush", &self.global_textures),
            ("census", &self.census),
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
        eprintln!("[gpu] pixel reads answered {}", self.pixel_reads);
        for (reason, count) in &self.pixel_reads_refused {
            eprintln!("[gpu] pixel reads refused: {reason}: {count}");
        }
        eprintln!("{}", self.screen_line());
        eprintln!(
            "[gpu] refused whole: {} frames or flushes with {} records",
            self.refused_works, self.refused_records
        );
        eprintln!(
            "[gpu] most storage held after a flush {} bytes, at any moment {} bytes",
            self.peak_live_bytes, self.peak_runtime_bytes
        );
        eprintln!(
            "[gpu] given back to make room: {} textures, {} render textures, {} meshes, {} bytes",
            self.reclaimed.textures,
            self.reclaimed.targets,
            self.reclaimed.meshes,
            self.reclaimed.bytes
        );
        for line in &self.refusal_holdings {
            eprintln!(
                "[gpu] held when the budget first refused, in frame {}: {line}",
                self.refusal_frame
            );
        }
        eprintln!(
            "[gpu] time in the frame path {:?}, in flushes {:?}",
            self.frame_time, self.flush_time
        );
        for (index, work) in self.work.iter().enumerate() {
            eprintln!("[gpu] {}", work.line(index));
        }
    }
}

/// A device and the player's offscreen host on it.
pub struct PlayerGpu {
    offscreen: Rc<RefCell<OffscreenHost>>,
    report: Rc<RefCell<GpuWorkReport>>,
    authority: SubmissionAuthority,
    device: wgpu::Device,
    /// The newest image of the screen read back, if work has drawn into it.
    screen: RefCell<Option<ScreenImage>>,
    /// The frame whose operations went to the viewer and have not been taken as submitted yet.
    handed: Cell<Option<u64>>,
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
        let wanted = player_device(&adapter);
        let (device, queue, _) = pollster::block_on(request_device(&adapter, &wanted.requirements))
            .map_err(|error| format!("no device: {error}"))?;
        let info = adapter.get_info();
        Self::build(
            device,
            SubmissionAuthority::new(queue),
            AllocationBudget(wanted.choice.bytes),
            data,
            &format!("adapter {} ({:?})", info.name, info.backend),
            wanted.choice.source,
        )
    }

    /// The host on the one device `source` makes for a window: the adapter's requirements and
    /// budget come from the same function the headless player uses, and the device meets the
    /// viewer's own needs as well. The window's frames and this player's work then share the
    /// device and its submission order.
    ///
    /// # Errors
    /// Fails with the reason when `source` has no adapter or cannot make the device, or the
    /// program runtime cannot reserve its arena on it.
    pub(crate) fn shared(source: &mut dyn DeviceSource, data: &Path) -> Result<Self, String> {
        let adapter = source.adapter()?;
        let wanted = player_device(&adapter);
        let (device, authority) = source.device(&wanted.requirements)?;
        let info = adapter.get_info();
        Self::build(
            device,
            authority,
            AllocationBudget(wanted.choice.bytes),
            data,
            &format!(
                "adapter {} ({:?}), shared with the viewer",
                info.name, info.backend
            ),
            wanted.choice.source,
        )
    }

    /// The host on a device and submission authority the caller already has, such as the
    /// viewer's, so the player's offscreen work and the caller's frames share one device and one
    /// submission order. The runtime may hold `budget` bytes of storage. The device keeps the
    /// limits and features it was made with: the arena fits its limits, and work it cannot run is
    /// left out by name, as on an adapter that lacks a feature.
    ///
    /// # Errors
    /// Fails with the reason when the program runtime cannot reserve its arena on the device.
    pub fn on(
        device: wgpu::Device,
        authority: SubmissionAuthority,
        budget: AllocationBudget,
        data: &Path,
    ) -> Result<Self, String> {
        Self::build(
            device,
            authority,
            budget,
            data,
            "the caller's device",
            "caller",
        )
    }

    /// The runtime, its arena and the host on `device`, with the report naming the device as
    /// `origin` and the budget as coming from `source`.
    fn build(
        device: wgpu::Device,
        authority: SubmissionAuthority,
        budget: AllocationBudget,
        data: &Path,
        origin: &str,
        source: &str,
    ) -> Result<Self, String> {
        let runtime = ProgramRuntime::new(device.clone(), authority.clone(), budget);
        let limits = device.limits();
        let mut renderer = Configuration::new(
            &authority,
            (),
            AllocationSet::default(),
            &limits,
            AllocationBudget(budget.0.saturating_mul(4)),
        )
        .map_err(|error| format!("no renderer configuration: {error}"))?;
        let reservation = renderer
            .reserve::<()>(
                &limits,
                ProgramRuntime::arena_allocations_within(budget.0, &limits),
            )
            .map_err(|error| format!("no arena reservation: {error:?}"))?;
        let arena = runtime
            .open_arena(reservation)
            .map_err(|error| format!("no arena: {error}"))?;
        let device_line = format!(
            "{origin}, buffer limits: max_buffer_size {}, max_uniform_buffer_binding_size {}, \
             max_storage_buffer_binding_size {}, min_uniform_buffer_offset_alignment {}, runtime \
             budget {} ({source})",
            limits.max_buffer_size,
            limits.max_uniform_buffer_binding_size,
            limits.max_storage_buffer_binding_size,
            limits.min_uniform_buffer_offset_alignment,
            budget.0,
        );
        let settings = Settings::cameras();
        let offscreen = OffscreenHost::new(
            runtime,
            arena,
            OffscreenAdapter::new(data, settings),
            ComputeAdapter::new(PlayerKernels::new(data)),
        );
        Ok(Self {
            offscreen: Rc::new(RefCell::new(offscreen)),
            report: Rc::new(RefCell::new(GpuWorkReport {
                device: device_line,
                ..GpuWorkReport::default()
            })),
            authority,
            device,
            screen: RefCell::new(None),
            handed: Cell::new(None),
        })
    }

    /// Sets the size of the screen a `CameraTarget` with no camera texture draws into, or takes
    /// the screen away with `None`. The next frame whose work names it makes it at that size,
    /// empty, so a window calls this when it is resized.
    pub fn set_screen(&self, size: Option<[u32; 2]>) {
        self.offscreen.borrow_mut().set_screen(size);
        if size.is_none() {
            *self.screen.borrow_mut() = None;
        }
    }

    /// The newest image of the screen: what it held after the last frame (or the flush before
    /// it) whose work drew into it, `None` before any did. Its `frame` says which frame that was,
    /// counting from zero, so a caller shows an image once by remembering the number.
    #[must_use]
    pub fn screen(&self) -> Option<std::cell::Ref<'_, ScreenImage>> {
        std::cell::Ref::filter_map(self.screen.borrow(), Option::as_ref).ok()
    }

    /// Reads the screen back when work drew into it since the last read, and keeps the image.
    /// A readback the host refuses is counted by its reason, and tried again by the next frame.
    /// The image is numbered `frame`, the frame whose work drew it.
    fn read_screen(&self, report: &mut GpuWorkReport, frame: u64) {
        let mut host = self.offscreen.borrow_mut();
        if !host.screen_written() {
            return;
        }
        match host.read_screen(frame) {
            Ok(image) => {
                report.screen_reads += 1;
                *self.screen.borrow_mut() = Some(image);
            }
            Err(reason) => *report.screen_reads_refused.entry(reason).or_default() += 1,
        }
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
            report.peak_runtime_bytes = offscreen.borrow().peak_bytes();
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

    /// What the run's offscreen work did so far.
    #[must_use]
    pub fn report(&self) -> std::cell::Ref<'_, GpuWorkReport> {
        self.report.borrow()
    }
}

#[cfg(test)]
#[path = "gpu_player_tests.rs"]
mod tests;
