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
//!   waits for the submission, and writes the changed buffers back before the read returns.
//!
//! The flush handler reaches the engine only through [`GpuFlush`], which does not read meshes
//! and textures. It reads the ones [`PlayerGpu::frame`] and its own earlier flushes asked for
//! from an [`AssetCache`] that the frame loop fills from the host between advances, so a mesh or
//! texture the very first flush names is left out of that flush as `MeshUnavailable` or
//! `SourceUnavailable` and is there for the next.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::{
    DisunityHost, GpuFlush, MeshChannels, MeshObjectView, MeshSlot, PlayerTexture, TextureSampler,
};
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;

use crate::PlayError;
use crate::offscreen::{
    AssetError, Assets, ComputeAdapter, OffscreenAdapter, OffscreenHost, PlayerKernels, Report,
    Settings,
};

/// Bytes of GPU storage the program runtime may hold.
const RUNTIME_BUDGET: u64 = 512 << 20;

/// The `LightMode` values a camera draws, in the order of preference.
const LIGHT_MODES: [&str; 5] = [
    "UniversalForward",
    "UniversalForwardOnly",
    "SRPDefaultUnlit",
    "ForwardBase",
    "Always",
];

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

/// The meshes and textures the host read for the flush handler, which cannot read them itself.
#[derive(Default)]
pub struct AssetCache {
    meshes: BTreeMap<u64, MeshChannels>,
    textures: BTreeMap<u64, PlayerTexture>,
    samplers: BTreeMap<u64, TextureSampler>,
    wanted_meshes: RefCell<BTreeSet<u64>>,
    wanted_textures: RefCell<BTreeSet<u64>>,
}

impl Assets for AssetCache {
    fn mesh(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, AssetError> {
        match self.meshes.get(&identity) {
            Some(held) if held.generation != known_generation => Ok(Some(held.clone())),
            Some(_) => Ok(None),
            None => {
                self.wanted_meshes.borrow_mut().insert(identity);
                Ok(None)
            }
        }
    }

    fn texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, AssetError> {
        match self.textures.get(&identity) {
            Some(held) if held.generation != known_generation => Ok(Some(held.clone())),
            Some(_) => Ok(None),
            None => {
                self.wanted_textures.borrow_mut().insert(identity);
                Ok(None)
            }
        }
    }

    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(self.samplers.get(&identity).copied())
    }
}

impl AssetCache {
    /// Wants the mesh of every presented object and the texture of every slot that has one, so a
    /// flush during the next advance finds them read. A flush handler cannot read the host, so
    /// whatever it first asks for would otherwise be missing from that flush.
    fn want_presented(&self, objects: &[MeshObjectView]) {
        let mut meshes = self.wanted_meshes.borrow_mut();
        let mut textures = self.wanted_textures.borrow_mut();
        for object in objects {
            meshes.insert(object.mesh);
            textures.extend(
                object
                    .slots
                    .iter()
                    .map(|slot: &MeshSlot| slot.texture)
                    .filter(|&texture| texture != 0),
            );
        }
    }

    /// Reads from `host` what the flush handler asked for since the last fill.
    fn fill(&mut self, host: &DisunityHost) {
        for identity in std::mem::take(&mut *self.wanted_meshes.borrow_mut()) {
            if let Ok(Some(mesh)) = host.mesh_channels(identity) {
                self.meshes.insert(identity, mesh);
            }
        }
        for identity in std::mem::take(&mut *self.wanted_textures.borrow_mut()) {
            if let Ok(Some(texture)) = host.player_texture(identity, 0) {
                self.textures.insert(identity, texture);
            }
            if let Ok(Some(sampler)) = host.texture_sampler(identity) {
                self.samplers.insert(identity, sampler);
            }
        }
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
    assets: Rc<RefCell<AssetCache>>,
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
        let settings = Settings {
            light_modes: LIGHT_MODES.map(str::to_owned).to_vec(),
        };
        let offscreen = OffscreenHost::new(
            runtime,
            arena,
            OffscreenAdapter::new(data, settings),
            ComputeAdapter::new(PlayerKernels::new(data)),
        );
        Ok(Self {
            offscreen: Rc::new(RefCell::new(offscreen)),
            assets: Rc::new(RefCell::new(AssetCache::default())),
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
        let assets = Rc::clone(&self.assets);
        let report = Rc::clone(&self.report);
        host.set_gpu_flush(Box::new(move |flush: &mut GpuFlush<'_>| {
            let started = Instant::now();
            let snapshot = flush.snapshot().map_err(|error| error.to_string())?;
            let outcome = offscreen
                .borrow_mut()
                .flush(&snapshot, &*assets.borrow())
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
        self.prefetch(host);
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

    /// Reads the meshes and textures the frame presents, ahead of the flush that may name them
    /// during the next advance. A host that presented nothing prefetches nothing.
    fn prefetch(&self, host: &DisunityHost) {
        let mut assets = self.assets.borrow_mut();
        if let Ok(objects) = host.mesh_objects() {
            assets.want_presented(&objects);
        }
        assets.fill(host);
    }

    /// Records the operations into a frame of their own, submits it and waits for it.
    fn run(
        &self,
        operations: worldrunner_render_program::offscreen::OffscreenOperations,
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

    fn presented(mesh: u64, textures: &[u64]) -> MeshObjectView {
        MeshObjectView {
            object: 1,
            mesh,
            local_to_world: [0.0; 16],
            slots: textures
                .iter()
                .map(|&texture| MeshSlot {
                    color: [1.0; 4],
                    texture,
                })
                .collect(),
        }
    }

    fn mesh(generation: u64) -> MeshChannels {
        MeshChannels {
            generation,
            vertex_count: 0,
            channels: Vec::new(),
            indices: Vec::new(),
            ranges: Vec::new(),
        }
    }

    #[test]
    fn a_mesh_the_cache_lacks_is_wanted_for_the_next_fill_and_a_held_one_is_given_per_generation() {
        let mut cache = AssetCache::default();
        assert_eq!(cache.mesh(7, 0).unwrap(), None);
        assert_eq!(
            cache
                .wanted_meshes
                .borrow()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [7]
        );
        cache.meshes.insert(7, mesh(3));
        assert_eq!(
            cache.mesh(7, 0).unwrap().map(|held| held.generation),
            Some(3)
        );
        assert_eq!(
            cache.mesh(7, 3).unwrap(),
            None,
            "unchanged since the adapter read it"
        );
        assert_eq!(
            cache.mesh(7, 2).unwrap().map(|held| held.generation),
            Some(3)
        );
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

    #[test]
    fn the_meshes_and_slot_textures_a_frame_presents_are_wanted_before_any_flush_asks() {
        let objects = [
            presented(7, &[11, 0]),
            presented(7, &[12]),
            presented(8, &[]),
        ];
        let cache = AssetCache::default();
        cache.want_presented(&objects);
        assert_eq!(
            cache
                .wanted_meshes
                .borrow()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [7, 8]
        );
        assert_eq!(
            cache
                .wanted_textures
                .borrow()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [11, 12],
            "a slot with no texture (0) wants none"
        );
    }

    #[test]
    fn a_texture_the_cache_lacks_is_wanted_and_has_no_sampler() {
        let cache = AssetCache::default();
        assert!(cache.texture(9, 0).unwrap().is_none());
        assert!(cache.sampler(9).unwrap().is_none());
        assert!(cache.wanted_textures.borrow().contains(&9));
    }
}
