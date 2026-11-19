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
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::{DisunityHost, GpuFlush};
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;

use crate::PlayError;
use crate::counts_line::{top_counts_line, unset_constants_line};
use crate::offscreen::{
    ComputeAdapter, DepthRange, DispatchTrace, OffscreenAdapter, OffscreenHost, PlayerKernels,
    Reclaimed, Report, STAND_INS_MAGENTA_ENV, Settings, StandInCounts, StandInLook, SuppliedGlobal,
    WorkTrace, supplied_line,
};
use crate::storage_budget::{self, Choice, STORAGE_BUDGET_ENV, host_available_bytes};

/// The most dispatch traces a report keeps.
const MAX_TRACES: usize = 32;

/// The most frame and flush traces a report keeps.
const MAX_WORK_TRACES: usize = 64;

/// The most textures the draw:MissingTexture line names.
const TOP_TEXTURES: usize = 5;

/// The most names the unset constants line gives.
const TOP_UNSET: usize = 8;

/// The most property and reason pairs the stand-ins line names.
const TOP_STAND_INS: usize = 8;

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
    /// The line saying which globals the run supplied, when it supplied any.
    pub supplied_globals: Option<String>,
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
    /// The draws that read a stand-in texture, over every piece of work, by property and reason.
    pub stand_ins: StandInCounts,
    /// The constant buffer members and fixed-state properties no layer set for a draw (see
    /// `Report::unset_constants`), over every piece of work.
    pub unset_constants: BTreeSet<String>,
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
        for (name, count) in &report.missing_channels {
            *self.missing_channels.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.defaulted_channels {
            *self.defaulted_channels.entry(name.clone()).or_default() += count;
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
        self.add_texture_counts(report);
        self.unset_constants.extend(report.unset_constants.clone());
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

    /// Adds the counts of the draws left out for want of a texture and of the draws and
    /// properties a stand-in served instead.
    fn add_texture_counts(&mut self, report: &Report) {
        for (name, count) in &report.missing_textures {
            *self.missing_textures.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.undeclared_defaults {
            *self.undeclared_defaults.entry(name.clone()).or_default() += count;
        }
        for (name, count) in &report.unresolved_textures {
            *self.unresolved_textures.entry(name.clone()).or_default() += count;
        }
        self.stand_ins.add(&report.stand_ins);
    }

    /// The lines of the stand-in census, without the `[gpu] ` prefix.
    fn stand_in_lines(&self) -> Vec<String> {
        self.stand_ins.lines(TOP_STAND_INS)
    }

    /// The lines about the draws left out for want of a texture and the draws that read stand-ins.
    fn print_texture_lines(&self) {
        if let Some(line) = top_counts_line(
            "draw:MissingTexture by texture",
            &self.missing_textures,
            TOP_TEXTURES,
        ) {
            eprintln!("[gpu] {line}");
        }
        for line in self.stand_in_lines() {
            eprintln!("[gpu] {line}");
        }
    }

    fn print_unset_constants(&self) {
        if let Some(line) = unset_constants_line(&self.unset_constants, TOP_UNSET) {
            eprintln!("[gpu] {line}");
        }
    }

    /// The lines that say what the run was given: the device and the globals it supplied.
    fn print_run_setup(&self) {
        eprintln!("[gpu] {}", self.device);
        if let Some(line) = &self.supplied_globals {
            eprintln!("[gpu] {line}");
        }
    }

    /// Writes the counts, one line each, to standard error.
    pub fn print(&self) {
        self.print_run_setup();
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
        self.print_texture_lines();
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
        self.print_unset_constants();
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

/// The storage budget for `adapter`: see [`storage_budget`].
fn storage_choice(adapter: &wgpu::Adapter) -> Choice {
    let env = std::env::var(STORAGE_BUDGET_ENV).ok();
    storage_budget::choose(
        env.as_deref(),
        adapter.get_info().device_type,
        host_available_bytes(),
    )
}

/// The device the headless player asks for: the default limits and block compression where
/// offered, with buffers as large as the storage budget allows (the adapter's own limit when that
/// is less, since the budget does not need one buffer to hold it). It prefers eight storage images in a stage where the adapter has them, because two of the Old
/// World's compute variants bind five and the default limit is four; an adapter that does not has
/// the default limit and leaves those two variants out by name.
fn device_requirements(budget: u64, available: &wgpu::Limits) -> DeviceRequirements {
    let default = wgpu::Limits::default();
    let required = wgpu::Limits {
        max_buffer_size: budget
            .max(default.max_buffer_size)
            .min(available.max_buffer_size),
        ..default
    };
    DeviceRequirements {
        // The player stores some textures block-compressed; a device without the feature leaves
        // them out by name. Unity filters its 32-bit float render textures; without
        // FLOAT32_FILTERABLE they are read at the nearest texel instead.
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::FLOAT32_FILTERABLE,
        optional_limits: Some(wgpu::Limits {
            max_storage_textures_per_shader_stage: PREFERRED_STORAGE_IMAGES,
            ..required.clone()
        }),
        required_limits: required,
        ..DeviceRequirements::default()
    }
}

/// The storage images a stage may bind that the device is asked for.
const PREFERRED_STORAGE_IMAGES: u32 = 8;

/// Builds every compute kernel variant of the player in `data` as a compute program on the
/// device the headless player asks for, and counts what the device cannot hold by cause.
///
/// # Errors
/// Fails with the reason when the machine has no adapter or device, or the player cannot be read.
pub fn compute_census(data: &Path) -> Result<(String, crate::offscreen::ComputeCensus), String> {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|error| format!("no adapter: {error}"))?;
    let budget = storage_choice(&adapter).bytes;
    let requirements = device_requirements(budget, &adapter.limits());
    let (device, queue, _) = pollster::block_on(request_device(&adapter, &requirements))
        .map_err(|error| format!("no device: {error}"))?;
    let name = format!(
        "{}, {} storage images in a stage",
        adapter.get_info().name,
        device.limits().max_storage_textures_per_shader_stage
    );
    let runtime = ProgramRuntime::new(
        device,
        SubmissionAuthority::new(queue),
        AllocationBudget(budget),
    );
    crate::offscreen::compute_census(data, &runtime)
        .map(|census| (name, census))
        .map_err(|error| error.to_string())
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

    /// Whether the host's render-program state has depth reversed.
    #[must_use]
    pub fn reversed_depth(&self) -> bool {
        self.offscreen.borrow().reversed_depth()
    }

    /// A device without a window and the host on it, with shaders loaded from the player in
    /// `data`, depth running as `depth` says, and the globals in `supplied` as fallbacks under
    /// the world's own (see `SuppliedGlobal`).
    ///
    /// # Errors
    /// Fails with the reason when the machine has no adapter, the device cannot be had or the
    /// program runtime cannot reserve its arena. The run goes on without GPU work then.
    pub fn headless(
        data: &Path,
        depth: DepthRange,
        supplied: &[SuppliedGlobal],
    ) -> Result<Self, String> {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|error| format!("no adapter: {error}"))?;
        let choice = storage_choice(&adapter);
        let requirements = device_requirements(choice.bytes, &adapter.limits());
        let (device, queue, _) = pollster::block_on(request_device(&adapter, &requirements))
            .map_err(|error| format!("no device: {error}"))?;
        let info = adapter.get_info();
        let look = StandInLook::from_setting(std::env::var(STAND_INS_MAGENTA_ENV).ok().as_deref());
        let settings = Settings {
            supplied: supplied.to_vec(),
            ..Settings::cameras()
                .with_depth(depth)
                .with_stand_in_look(look)
        };
        Self::build(
            device,
            SubmissionAuthority::new(queue),
            AllocationBudget(choice.bytes),
            data,
            &format!("adapter {} ({:?})", info.name, info.backend),
            choice.source,
            settings,
        )
    }

    /// The host on a device and submission authority the caller already has, such as the
    /// viewer's, so the player's offscreen work and the caller's frames share one device and one
    /// submission order. The runtime may hold `budget` bytes of storage. The device keeps the
    /// limits and features it was made with: the arena fits its limits, and work it cannot run is
    /// left out by name, as on an adapter that lacks a feature. Depth runs forward and no globals
    /// are supplied, as for any library caller that builds [`Settings`] itself.
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
            Settings::cameras(),
        )
    }

    /// The runtime, its arena and the host on `device`, with the report naming the device as
    /// `origin` and the budget as coming from `source`, drawing as `settings` say.
    fn build(
        device: wgpu::Device,
        authority: SubmissionAuthority,
        budget: AllocationBudget,
        data: &Path,
        origin: &str,
        source: &str,
        settings: Settings,
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
        let supplied_globals = supplied_line(&settings.supplied);
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
                supplied_globals,
                ..GpuWorkReport::default()
            })),
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
        report.peak_runtime_bytes = self.offscreen.borrow().peak_bytes();
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

    fn counted(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
        entries
            .iter()
            .map(|(name, count)| ((*name).to_owned(), *count))
            .collect()
    }

    fn with_stand_ins(draws: usize, by_property: &[(&str, usize)]) -> Report {
        Report {
            stand_ins: StandInCounts {
                draws,
                by_property: counted(by_property),
            },
            ..Report::default()
        }
    }

    #[test]
    fn the_summary_says_how_many_draws_read_stand_ins_and_which_properties_for_which_reason() {
        let mut report = GpuWorkReport::default();
        report.add(
            &with_stand_ins(3, &[("_A unset", 3), ("_B undeclared", 1)]),
            "frame",
        );
        report.add(
            &with_stand_ins(2, &[("_A unset", 1), ("_C exists-not-loaded", 2)]),
            "flush",
        );
        assert_eq!(
            report.stand_in_lines(),
            [
                "draws with stand-ins 5",
                "stand-ins by property: _A unset 4, _C exists-not-loaded 2, _B undeclared 1",
            ]
        );
    }

    #[test]
    fn a_run_with_no_stand_ins_says_zero_draws_and_names_no_property() {
        let mut report = GpuWorkReport::default();
        report.add(&Report::default(), "frame");
        assert_eq!(report.stand_in_lines(), ["draws with stand-ins 0"]);
    }

    fn names(entries: &[&str]) -> BTreeSet<String> {
        entries.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn the_run_report_keeps_each_unset_constant_once_over_every_piece_of_work() {
        let mut report = GpuWorkReport::default();
        let first = Report {
            unset_constants: names(&["_B", "_A"]),
            ..Report::default()
        };
        let second = Report {
            unset_constants: names(&["_A", "_C"]),
            ..Report::default()
        };
        report.add(&first, "frame");
        report.add(&second, "flush");
        assert_eq!(report.unset_constants, names(&["_A", "_B", "_C"]));
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

    /// The limits an adapter that offers `storage_images` storage images in a stage and buffers
    /// of `adapter_buffer` bytes would give the player's request for a budget of `budget`.
    fn negotiated(storage_images: u32, adapter_buffer: u64, budget: u64) -> wgpu::Limits {
        let available = wgpu::Limits {
            max_buffer_size: adapter_buffer,
            max_storage_textures_per_shader_stage: storage_images,
            ..wgpu::Limits::default()
        };
        worldrunner_render_gpu::device::negotiate(
            wgpu::Features::empty(),
            &available,
            &device_requirements(budget, &available),
        )
        .unwrap()
        .limits
    }

    #[test]
    fn the_headless_device_asks_for_the_storage_images_the_old_world_kernels_bind_when_offered() {
        let budget = storage_budget::DEFAULT_BYTES;
        // Two Old World compute variants bind five storage images in their one stage, one more
        // than the default limit of four.
        let offered = negotiated(16, budget * 2, budget);
        assert!(offered.max_storage_textures_per_shader_stage >= 5);
        assert_eq!(offered.max_buffer_size, budget);
        assert_eq!(
            negotiated(4, budget * 2, budget).max_storage_textures_per_shader_stage,
            4,
            "an adapter at the default limit still gets a device"
        );
    }

    #[test]
    fn an_adapter_whose_buffers_are_smaller_than_the_budget_still_gets_a_device() {
        // The budget is a total, not a buffer: the device takes the adapter's largest buffer.
        let default = wgpu::Limits::default().max_buffer_size;
        let limits = negotiated(16, default, 4 * default);
        assert_eq!(limits.max_buffer_size, default);
        assert!(limits.max_storage_textures_per_shader_stage >= 5);
    }

    #[test]
    fn the_headless_host_draws_with_the_depth_range_it_is_given() {
        let directory = tempfile::tempdir().unwrap();
        for depth in [DepthRange::Forward, DepthRange::Reversed] {
            let gpu = PlayerGpu::headless(directory.path(), depth, &[]).unwrap();
            assert_eq!(gpu.reversed_depth(), depth.reversed(), "{depth:?}");
        }
    }

    #[test]
    fn the_report_says_which_globals_the_run_supplied_and_says_nothing_when_it_supplied_none() {
        let directory = tempfile::tempdir().unwrap();
        let none = PlayerGpu::headless(directory.path(), DepthRange::Forward, &[]).unwrap();
        assert_eq!(none.report().supplied_globals, None);
        let value = SuppliedGlobal::parse("unity_LightData=1,1,1,1").unwrap();
        let some = PlayerGpu::headless(directory.path(), DepthRange::Forward, &[value]).unwrap();
        assert_eq!(
            some.report().supplied_globals.as_deref(),
            Some("globals supplied for this run: unity_LightData=(1, 1, 1, 1)")
        );
    }

    #[test]
    fn the_headless_device_takes_the_budget_the_chooser_gives_its_adapter() {
        let directory = tempfile::tempdir().unwrap();
        let gpu = PlayerGpu::headless(directory.path(), DepthRange::Forward, &[]).unwrap();
        assert!(gpu.storage_budget() >= storage_budget::DEFAULT_BYTES);
        assert!(
            gpu.report().device.contains("runtime budget"),
            "the report says where the budget came from"
        );
    }

    #[test]
    fn a_player_on_a_callers_device_submits_through_the_callers_authority() {
        let shared =
            worldrunner_render_gpu::test_support::TestGpu::shared(&DeviceRequirements::default())
                .unwrap();
        let authority = SubmissionAuthority::new(shared.queue.clone());
        let directory = tempfile::tempdir().unwrap();
        let budget = AllocationBudget(storage_budget::DEFAULT_BYTES);
        let gpu = PlayerGpu::on(
            shared.device.clone(),
            authority.clone(),
            budget,
            directory.path(),
        )
        .unwrap();
        assert!(gpu.authority.same_authority(&authority));
        assert!(
            gpu.offscreen
                .borrow()
                .authority()
                .same_authority(&authority),
            "the program runtime records and submits through the caller's authority"
        );
        assert!(
            !gpu.offscreen
                .borrow()
                .authority()
                .same_authority(&SubmissionAuthority::new(shared.queue.clone())),
            "a second authority on the same queue is a different one"
        );
        assert_eq!(gpu.storage_budget(), budget.0);
        assert!(gpu.report().device.contains("runtime budget"));
    }
}
