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
//!   its own frame's submission. The operations that draw into the screen export it, and the
//!   window shows the screen's texture itself (`PlayerGpu::game_screen`): nothing is read back.
//! * When a script reads a compute buffer back, the managed engine calls the flush handler
//!   [`PlayerGpu::install`] registers. It runs the work recorded so far, dispatches included,
//!   waits for the submission, and writes the changed buffers back before the read returns. It
//!   reads meshes, textures and samplers through [`GpuFlush`], so one the flush names for the
//!   first time is there for that flush.
//!

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_disunity_runtime::GpuFlush;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::configuration::Configuration;
use worldrunner_render_gpu::device::request_device;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;
use worldrunner_render_program::unity::offscreen::ListDraws;
use worldrunner_scene_viewer::GameScreen;

mod frame;
mod refusal;
mod ui_offer;
pub(crate) use ui_offer::UiOffer;

use crate::PlayError;
use crate::counts_line::names_line;
use crate::counts_line::top_counts_line;
use crate::global_listing::GlobalListing;
use crate::gpu_device::DeviceSource;
use crate::gpu_device::PlayerStartupInputs;
use crate::gpu_device::headless_gpu_instance;
use crate::gpu_device::player_device;
use crate::offscreen::ComputeAdapter;
use crate::offscreen::DispatchTrace;
use crate::offscreen::GlobalReads;
use crate::offscreen::OffscreenAdapter;
use crate::offscreen::OffscreenHost;
use crate::offscreen::Phases;
use crate::offscreen::PlayerKernels;
use crate::offscreen::Reclaimed;
use crate::offscreen::Report;
use crate::offscreen::Settings;
use crate::offscreen::StandInCounts;
use crate::offscreen::StandInLook;
use crate::offscreen::SuppliedGlobal;
use crate::offscreen::WorkTrace;
use crate::offscreen::supplied_line;
use crate::scene_census::SubstitutionCensus;

/// The most dispatch traces a report keeps.
const MAX_TRACES: usize = 32;

/// The most frame and flush traces a report keeps.
const MAX_WORK_TRACES: usize = 64;

/// The most textures the draw:MissingTexture line names.
const TOP_TEXTURES: usize = 5;

/// The most names the unset constants line gives.
const TOP_UNSET: usize = 8;

/// The most names the line of globals no draw read gives.
const TOP_UNREAD_GLOBALS: usize = 30;

/// The most property and reason pairs the stand-ins line names.
const TOP_STAND_INS: usize = 8;

/// What a run asks of its offscreen work besides the player's files.
#[derive(Clone, Debug, Default)]
pub struct GpuLaunch {
    /// The globals the run supplies by name, which its GPU work starts every frame from.
    pub globals: Vec<SuppliedGlobal>,
    /// How the stand-in textures the host supplies look.
    pub stand_ins: StandInLook,
    /// Whether a headless run prints the GPU report after every frame, not only at the end.
    pub report_each_frame: bool,
    /// The records whose targets the run writes to files.
    pub reads: Vec<crate::TargetRead>,
}

impl GpuLaunch {
    /// The settings of the run's offscreen work: `globals` as the fallback globals every frame
    /// starts from, with stand-in textures drawn as `stand_ins` says.
    fn settings(&self) -> Settings {
        Settings {
            supplied: self.globals.clone(),
            ..Settings::cameras().with_stand_in_look(self.stand_ins)
        }
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
    /// What built frames did that is worth reading and is not a failure.
    pub notes: BTreeMap<String, usize>,
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
    /// The first refusal the run has not been told of yet (see `PlayerGpu::take_refusal`).
    refusal: Option<String>,
    /// Why a frame's work or a flush failed, by message.
    pub errors: BTreeMap<String, usize>,
    /// The `--read-target` reads that wrote no file.
    pub reads_unwritten: usize,
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
    /// The draws left out because no layer names a structured buffer the pass reads, by the property.
    pub missing_buffers: BTreeMap<String, usize>,
    /// The draws left out because the mesh lacks a vertex channel the pass reads.
    pub missing_channels: BTreeMap<String, usize>,
    /// The draws that read a constant for a vertex channel their mesh lacks, which are recorded.
    pub defaulted_channels: BTreeMap<String, usize>,
    /// The texture properties that named no texture the frame can read and read no stand-in, by
    /// `name: reason`. A property that reads a stand-in is counted in `stand_ins`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// The draws that read a stand-in texture, over every piece of work, by property and reason.
    pub stand_ins: StandInCounts,
    /// The texture globals the run set and the names its draws looked up (see `GlobalReads`).
    pub global_reads: GlobalReads,
    /// The retained renderers drawn white and left out, when the run drew a retained scene.
    pub retained: Option<SubstitutionCensus>,
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
    /// The builds, the draws they held and the texture views they made, over every piece of work.
    pub planner: Phases,
}

impl GpuWorkReport {
    pub(crate) fn keep_traces(&mut self, traces: Vec<WorkTrace>) {
        for trace in traces.iter().filter(|trace| trace.error.is_some()) {
            self.refused_records += trace.records;
            self.refused_works += 1;
            refusal::note_whole(&mut self.refusal, trace);
        }
        for trace in &traces {
            self.planner.add(&trace.phases);
        }
        let room = MAX_WORK_TRACES.saturating_sub(self.work.len());
        self.work.extend(traces.into_iter().take(room));
    }

    fn add(&mut self, report: &Report, source: &'static str) {
        refusal::note_over_budget(&mut self.refusal, report, source);
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
        for (name, count) in &report.notes {
            *self.notes.entry(name.clone()).or_default() += count;
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
        for (name, count) in &report.missing_buffers {
            *self.missing_buffers.entry(name.clone()).or_default() += count;
        }
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
            .map(ListDraws::left_out)
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
        for (name, count) in &report.unresolved_textures {
            *self.unresolved_textures.entry(name.clone()).or_default() += count;
        }
        self.stand_ins.add(&report.stand_ins);
        self.global_reads.add(&report.global_reads);
    }

    /// The lines of the stand-in census, without the `[gpu] ` prefix.
    fn stand_in_lines(&self) -> Vec<String> {
        self.stand_ins.lines(TOP_STAND_INS)
    }

    /// The lines of the retained renderer census, without the `[gpu] ` prefix.
    fn retained_lines(&self) -> Vec<String> {
        self.retained
            .as_ref()
            .map_or_else(Vec::new, SubstitutionCensus::lines)
    }

    /// The lines of the stand-in census of the work done since the census was `before`.
    fn frame_stand_in_lines(&self, before: &StandInCounts) -> Vec<String> {
        self.stand_ins.since(before).lines(TOP_STAND_INS)
    }

    /// The lines about the draws left out for want of a texture, from `stand_in_lines` the
    /// draws that read stand-ins, and the retained renderers drawn white.
    fn print_texture_lines(&self, stand_in_lines: &[String]) {
        if let Some(line) = top_counts_line(
            "draw:MissingTexture by texture",
            &self.missing_textures,
            TOP_TEXTURES,
        ) {
            eprintln!("[gpu] {line}");
        }
        for line in stand_in_lines.iter().chain(&self.retained_lines()) {
            eprintln!("[gpu] {line}");
        }
        if let Some(line) = self.global_reads.line(TOP_UNREAD_GLOBALS) {
            eprintln!("[gpu] {line}");
        }
    }

    fn print_unset_constants(&self) {
        if let Some(line) = names_line(
            "constants or state properties no layer set",
            &self.unset_constants,
            TOP_UNSET,
        ) {
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

    /// The lines about the storage the run held, gave back and refused, and the time it took.
    fn print_storage_lines(&self) {
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
        eprintln!("[gpu] planner: {}", self.planner.planner_line());
        eprintln!(
            "[gpu] time in the frame path {:?}, in flushes {:?}",
            self.frame_time, self.flush_time
        );
    }

    /// Writes the counts, one line each, to standard error. With `since`, a clone of `stand_ins`
    /// taken at some point, the stand-in census is that of the work done since then, in place of
    /// the run's totals.
    pub fn print(&self, since: Option<&StandInCounts>) {
        let stand_in_lines = since.map_or_else(
            || self.stand_in_lines(),
            |before| self.frame_stand_in_lines(before),
        );
        self.print_run_setup();
        eprintln!(
            "[gpu] frames with work {}, flushes {}, records {}, built {}, dispatches {}",
            self.frames_with_work, self.flushes, self.records, self.built, self.dispatches
        );
        for (title, counts) in [
            ("skipped records", &self.skipped),
            ("skipped renderers", &self.renderers_skipped),
            ("failures", &self.failures),
            ("notes", &self.notes),
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
        self.print_texture_lines(&stand_in_lines);
        for (title, counts) in [
            ("draws without a pass", &self.unmatched_passes),
            ("partial texture reads", &self.partial_reads),
            ("draw:MissingBuffer by buffer", &self.missing_buffers),
            ("draws without a mesh channel", &self.missing_channels),
            ("draw:DefaultedChannel", &self.defaulted_channels),
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
        self.print_storage_lines();
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
    /// The frame whose work (or a flush before it) last drew into the screen, counting from zero;
    /// `None` before any did.
    drawn_screen: Cell<Option<u64>>,
    /// The size the screen has been asked to be, which a texture of another size no longer is.
    screen_size: Cell<Option<[u32; 2]>>,
    /// The size of the layer atlas the last frame's UI work draws into, `None` for a frame with no
    /// UI work.
    ui_atlas: Cell<Option<[u32; 2]>>,
    /// The reads of a record's target the run was asked for, which the frames they name make.
    reads: RefCell<Vec<crate::TargetRead>>,
}

impl PlayerGpu {
    /// Bytes of GPU storage the runtime may hold.
    #[must_use]
    pub fn storage_budget(&self) -> u64 {
        self.offscreen.borrow().storage_budget()
    }

    /// A device without a window and the host on it, with shaders loaded from the player in
    /// `data`, and the run's globals as fallbacks under the world's own (see `SuppliedGlobal`),
    /// stand-in look and storage budget as `launch` says.
    ///
    /// # Errors
    /// Fails with the reason when the machine has no adapter, the device cannot be had or the
    /// program runtime cannot reserve its arena.
    pub fn headless(data: &Path, launch: &GpuLaunch) -> Result<Self, String> {
        let inputs = PlayerStartupInputs::capture();
        let settings = launch.settings();
        let instance = headless_gpu_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|error| format!("no adapter: {error}"))?;
        Self::headless_on_adapter(&adapter, data, &inputs, settings)
    }

    fn headless_on_adapter(
        adapter: &wgpu::Adapter,
        data: &Path,
        inputs: &PlayerStartupInputs,
        settings: Settings,
    ) -> Result<Self, String> {
        let wanted = player_device(adapter, inputs);
        let (device, queue, _) = pollster::block_on(request_device(adapter, &wanted.requirements))
            .map_err(|error| format!("no device: {error}"))?;
        let info = adapter.get_info();
        Self::build(
            device,
            SubmissionAuthority::new(queue),
            AllocationBudget(wanted.choice.bytes),
            data,
            &format!("adapter {} ({:?})", info.name, info.backend),
            wanted.choice.source,
            settings,
        )
    }

    /// The host on the one device `source` makes for a window: the adapter's requirements and
    /// budget come from the same function the headless player uses, and the device meets the
    /// viewer's own needs as well. The window's frames and this player's work then share the
    /// device and its submission order. `launch` means what it does for
    /// [`PlayerGpu::headless`].
    ///
    /// # Errors
    /// Fails with the reason when `source` has no adapter or cannot make the device, or the
    /// program runtime cannot reserve its arena on it.
    pub(crate) fn shared(
        source: &mut dyn DeviceSource,
        data: &Path,
        launch: &GpuLaunch,
    ) -> Result<Self, String> {
        let inputs = PlayerStartupInputs::capture();
        let settings = launch.settings();
        let adapter = source.adapter()?;
        Self::shared_on_adapter(source, &adapter, data, &inputs, settings)
    }

    fn shared_on_adapter(
        source: &mut dyn DeviceSource,
        adapter: &wgpu::Adapter,
        data: &Path,
        inputs: &PlayerStartupInputs,
        settings: Settings,
    ) -> Result<Self, String> {
        let wanted = player_device(adapter, inputs);
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
            drawn_screen: Cell::new(None),
            screen_size: Cell::new(None),
            ui_atlas: Cell::new(None),
            reads: RefCell::new(Vec::new()),
        })
    }

    /// Sets the size of the screen a `CameraTarget` with no camera texture draws into, or takes
    /// the screen away with `None`. The next frame whose work names it makes it at that size,
    /// empty, so a window calls this when it is resized.
    pub fn set_screen(&self, size: Option<[u32; 2]>) {
        self.screen_size.set(size);
        self.offscreen.borrow_mut().set_screen(size);
        if size.is_none() {
            self.drawn_screen.set(None);
        }
    }

    /// Asks for the reads of a record's target, each made by the player's own frame it names.
    pub fn set_reads(&self, reads: Vec<crate::TargetRead>) {
        *self.reads.borrow_mut() = reads;
    }

    /// The number of the last frame whose work (or a flush before it) drew into the screen,
    /// counting from zero, `None` before any did.
    #[must_use]
    pub fn drawn_screen(&self) -> Option<u64> {
        self.drawn_screen.get()
    }

    /// The screen as the window shows it: the texture the offscreen work draws into, with whether
    /// a flush the player submitted itself already wrote it. A frame's work handed to the viewer
    /// is not counted: the viewer knows whether it ran. Nothing is copied. `None` while there is
    /// no screen, before any frame made its texture, and when the texture is not the size the
    /// screen was last asked to be: a resize makes the next frame that has work a new one, and the
    /// old texture shows a size the window no longer has. A texture the viewer cannot sample is
    /// counted in the report and shows nothing.
    pub(crate) fn game_screen(&self) -> Option<GameScreen> {
        let host = self.offscreen.borrow();
        let (texture, flushed) = host.screen_texture()?;
        if self.screen_size.get() != Some([texture.width(), texture.height()]) {
            return None;
        }
        GameScreen::new(texture, flushed)
            .map_err(|error| {
                *self
                    .report
                    .borrow_mut()
                    .errors
                    .entry(error.to_string())
                    .or_default() += 1;
            })
            .ok()
    }

    /// The texture the UI frame's runs were drawn into, which the viewer's overlay shows. `None`
    /// until a frame with runs made it, after a frame without any, and while it is not the size
    /// of the atlas that frame planned: a resize or a plan that needs a larger atlas makes the
    /// next frame that has runs a new texture.
    pub(crate) fn ui_layers(&self) -> Option<wgpu::Texture> {
        let texture = self.offscreen.borrow().ui_layers_texture()?;
        (self.ui_atlas.get() == Some([texture.width(), texture.height()])).then_some(texture)
    }

    /// The largest side of a texture the player's device can make.
    pub(crate) fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Numbers the screen `frame` when work drew into it since the last frame.
    fn note_drawn_screen(&self, frame: u64) {
        if self.offscreen.borrow_mut().take_screen_written() {
            self.drawn_screen.set(Some(frame));
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

    /// Records the retained scene's census of renderers drawn white, for the report.
    pub fn note_retained(&self, census: SubstitutionCensus) {
        self.report.borrow_mut().retained = Some(census);
    }

    /// What the run's offscreen work did so far.
    #[must_use]
    pub fn report(&self) -> std::cell::Ref<'_, GpuWorkReport> {
        self.report.borrow()
    }
}

#[cfg(test)]
#[path = "gpu_player_retained_tests.rs"]
mod retained_tests;
#[cfg(test)]
mod stand_in_tests;

#[cfg(test)]
#[path = "gpu_player_tests.rs"]
mod tests;
