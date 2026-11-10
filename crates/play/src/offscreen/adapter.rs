//! The adapter a host calls each frame: it loads and uploads what a snapshot needs, maps the
//! snapshot onto an [`OffscreenFrame`] and builds the frame's operations.
//!
//! Shaders, meshes and stored textures are registered with the [`OffscreenState`] once and again
//! when they change. A shader is registered again when its world shader's source changes, and a
//! mesh or texture when the managed engine's generation of it rises.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, MeshIndices, OffscreenFrame, Record, RenderTextureSpec,
    SubMesh,
};
use worldrunner_render_program::unity::offscreen::{
    OffscreenError, OffscreenOperations, OffscreenState,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, ShaderId as WorldShaderId, ShaderSource, TextureRef,
};

use super::assets::{Assets, MeshError, mesh_of, sampler_of_stored, texture_of};
use super::bounds::Bounds;
use super::defaults::DefaultTexture;
use super::frame::{
    Adapted, Available, BLIT_MESH, PASS_TAG, Report, Settings, ShaderInfo, adapt_probing,
    textures_of,
};
use super::holdings;
use super::ids;
use super::script::temporary_ids;
use super::shaders::{ShaderLoader, ShaderSupply};
use worldrunner_world_simulation::graphics::GraphicsSnapshot;

mod reclaim;

/// What the adapter keeps between frames.
pub struct OffscreenAdapter {
    supply: Box<dyn ShaderSupply>,
    settings: Settings,
    shaders: BTreeMap<WorldShaderId, (ShaderSource, ShaderInfo)>,
    meshes: BTreeMap<u64, Stored>,
    /// The box of each uploaded mesh's vertices.
    bounds: BTreeMap<u64, Bounds>,
    /// The index count of each sub-mesh of each uploaded mesh, by range number.
    sub_meshes: BTreeMap<u64, Vec<u32>>,
    textures: BTreeMap<u64, Stored>,
    /// The default textures that were registered.
    defaults: BTreeSet<DefaultTexture>,
    blit_mesh: bool,
    /// Whether a block-compressed texture goes up as its blocks when the device samples them.
    block_sources: bool,
    /// Whether the changed test renders have been made once; they cost storage, so they are made
    /// for the first probed work only.
    variants_made: bool,
    /// Counts the snapshots adapted, so a stored mesh or texture knows how long ago it was named.
    epoch: u64,
    /// The epoch of the last snapshot or frame that named each render texture the state may hold.
    targets: BTreeMap<TextureId, u64>,
}

/// A stored mesh or texture the adapter has read: the generation, the generation of the sampler
/// it was uploaded with (0 for a mesh), and whether it can be drawn.
#[derive(Clone, Copy)]
struct Stored {
    generation: u64,
    sampler_generation: u64,
    usable: bool,
    /// Whether it was read and has nothing to draw, which is not a failure (a mesh only).
    empty: bool,
    /// The epoch of the last snapshot that named it.
    last_named: u64,
}

impl OffscreenAdapter {
    /// An adapter that loads shaders from the player's `data_directory`.
    #[must_use]
    pub fn new(data_directory: impl Into<PathBuf>, settings: Settings) -> Self {
        Self::with_supply(ShaderLoader::new(data_directory), settings)
    }

    /// An adapter that gets its shaders from `supply`.
    #[must_use]
    pub fn with_supply(supply: impl ShaderSupply + 'static, settings: Settings) -> Self {
        Self {
            supply: Box::new(supply),
            settings,
            shaders: BTreeMap::new(),
            meshes: BTreeMap::new(),
            bounds: BTreeMap::new(),
            sub_meshes: BTreeMap::new(),
            textures: BTreeMap::new(),
            defaults: BTreeSet::new(),
            blit_mesh: false,
            block_sources: true,
            variants_made: false,
            epoch: 0,
            targets: BTreeMap::new(),
        }
    }

    /// Chooses whether block-compressed textures go up as blocks (the default, on a device that
    /// samples them) or decoded to RGBA8.
    #[must_use]
    pub fn with_block_sources(mut self, blocks: bool) -> Self {
        self.block_sources = blocks;
        self
    }

    #[cfg(test)]
    pub(super) fn mesh_metadata_counts(&self) -> (usize, usize, usize) {
        (self.meshes.len(), self.bounds.len(), self.sub_meshes.len())
    }

    /// Registers what the snapshot needs with `state`, then maps it. What cannot be loaded or
    /// uploaded is counted in the report's failures and left out of the frame.
    pub fn adapt(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
    ) -> Adapted {
        self.adapt_probing(snapshot, assets, state, runtime, false)
    }

    /// As [`adapt`](Self::adapt); with `probe`, the camera renders into the render textures a
    /// dispatch binds are examined ([`adapt_probing`]).
    pub fn adapt_probing(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        probe: bool,
    ) -> Adapted {
        self.epoch += 1;
        let mut notes = Report::default();
        let mut available = Available::default();
        // A render texture the script destroyed has nothing left to draw; its storage goes back
        // to the budget before this snapshot's textures are made.
        let mut live_targets: BTreeSet<_> = snapshot
            .render_textures
            .keys()
            .map(|id| ids::render_texture(*id))
            .collect();
        live_targets.extend(temporary_ids(&snapshot.work));
        state.retain_render_textures(runtime, &live_targets);
        self.stamp_targets(reclaim::named_targets(snapshot), &live_targets);
        self.register_shaders(snapshot, state, &mut available, &mut notes);
        let meshes = named_meshes(snapshot);
        let wanted = stored_textures(snapshot);
        // What the snapshot names is stamped before anything is uploaded, so that making room for
        // one upload never gives back another the snapshot is about to use.
        self.stamp(&meshes, &wanted);
        self.register_meshes(meshes, assets, (state, runtime), &mut notes);
        self.register_defaults(state, runtime, &mut notes);
        self.register_textures(&wanted, assets, (state, runtime), &mut notes);
        available.meshes = usable(&self.meshes);
        available.bounds = self
            .bounds
            .iter()
            .filter(|(identity, _)| available.meshes.contains(identity))
            .map(|(identity, bounds)| (*identity, *bounds))
            .collect();
        available.sub_meshes = self
            .sub_meshes
            .iter()
            .filter(|(identity, _)| available.meshes.contains(identity))
            .map(|(identity, counts)| (*identity, counts.clone()))
            .collect();
        available.empty_meshes = self
            .meshes
            .iter()
            .filter(|(_, held)| held.empty)
            .map(|(identity, _)| *identity)
            .collect();
        available.textures = usable(&self.textures);
        available.default_textures = self.defaults.clone();
        available.test_variants = probe && !self.variants_made;
        if !self.blit_mesh {
            let blit = state.set_mesh(runtime, BLIT_MESH, &blit_mesh());
            self.blit_mesh = blit.is_ok();
        }
        let mut adapted = adapt_probing(snapshot, &available, &self.settings, probe);
        self.variants_made |= !adapted.frame.probe.variants.is_empty();
        for spec in &adapted.frame.render_textures {
            self.targets.insert(spec.id, self.epoch);
        }
        adapted.report.absorb_failures(notes);
        adapted
    }

    fn register_shaders(
        &mut self,
        snapshot: &GraphicsSnapshot,
        state: &mut OffscreenState,
        available: &mut Available,
        notes: &mut Report,
    ) {
        for (id, shader) in &snapshot.shaders {
            let Some(source) = &shader.source else {
                notes.count("shader:NoSource");
                continue;
            };
            if let Some((held, info)) = self.shaders.get(id)
                && held == source
            {
                available.shaders.insert(*id, info.clone());
                continue;
            }
            match self.supply.load(source) {
                Ok(loaded) => {
                    let (_, light_modes) = loaded.program.pass_tags(PASS_TAG);
                    for mode in &light_modes {
                        *notes
                            .census
                            .entry(format!("shader pass tag {PASS_TAG}={mode}"))
                            .or_default() += 1;
                    }
                    let info = ShaderInfo {
                        passes: loaded.passes,
                        light_modes,
                        queue: loaded.queue,
                        texture_defaults: loaded.texture_defaults,
                        fill: loaded.fill,
                    };
                    state.add_shader(ids::shader(*id), loaded.program);
                    self.shaders.insert(*id, (source.clone(), info.clone()));
                    available.shaders.insert(*id, info);
                }
                Err(error) => notes.fail("shader", &error),
            }
        }
    }

    /// Registers each default texture once.
    fn register_defaults(
        &mut self,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
        notes: &mut Report,
    ) {
        for texture in DefaultTexture::ALL {
            if self.defaults.contains(&texture) {
                continue;
            }
            match state.set_texture(runtime, &texture.texture()) {
                Ok(()) => {
                    self.defaults.insert(texture);
                }
                Err(error) => notes.fail("defaulttexture", &error),
            }
        }
    }

    fn register_meshes(
        &mut self,
        identities: BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) {
        for identity in identities {
            let known = self.meshes.get(&identity).map_or(0, |held| held.generation);
            let stored = match assets.mesh(identity, known) {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(_) => {
                    notes.count("mesh:Read");
                    continue;
                }
            };
            let mut nothing_to_draw = false;
            let uploaded = match mesh_of(&stored) {
                Ok(mesh) => {
                    match Bounds::of_mesh(&mesh) {
                        Some(bounds) => self.bounds.insert(identity, bounds),
                        None => self.bounds.remove(&identity),
                    };
                    self.sub_meshes.insert(
                        identity,
                        mesh.sub_meshes.iter().map(|sub| sub.count).collect(),
                    );
                    self.with_room(state, runtime, notes, |state, runtime| {
                        state.set_mesh(runtime, ids::mesh(identity), &mesh)
                    })
                    .map_err(|error| notes.fail("mesh", &error))
                    .is_ok()
                }
                Err(MeshError::Empty(empty)) => {
                    // Nothing to draw is not a failure. The mesh stays unusable, so renderers of
                    // it are skipped and a mesh draw of it is refused as empty.
                    nothing_to_draw = true;
                    *notes
                        .census
                        .entry(format!("mesh with nothing to draw: {empty}"))
                        .or_default() += 1;
                    false
                }
                Err(error) => {
                    notes.fail("mesh", &error);
                    false
                }
            };
            self.meshes.insert(
                identity,
                Stored {
                    generation: stored.generation,
                    sampler_generation: 0,
                    usable: uploaded,
                    empty: nothing_to_draw,
                    last_named: self.epoch,
                },
            );
        }
    }

    fn register_textures(
        &mut self,
        identities: &BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) {
        for &identity in identities {
            let Some(id) = ids::stored_texture(identity) else {
                notes.count("texture:IdentityTooLarge");
                continue;
            };
            let held = self.textures.get(&identity).copied();
            let stored_sampler = assets.sampler(identity).unwrap_or_else(|_| {
                notes.count("texture:SamplerRead");
                None
            });
            // A sampler that changed alone replaces the texture, whose bytes are read again.
            let sampler_changed = stored_sampler.as_ref().is_some_and(|found| {
                held.is_none_or(|held| held.sampler_generation != found.generation)
            });
            let known = held
                .filter(|_| !sampler_changed)
                .map_or(0, |held| held.generation);
            let stored = match assets.texture(identity, known) {
                Ok(Some(stored)) => stored,
                // None is a texture the engine does not hold, or one it holds unchanged.
                Ok(None) => {
                    if held.is_none() {
                        notes.count("texture:NotStored");
                    }
                    continue;
                }
                Err(_) => {
                    notes.count("texture:Read");
                    continue;
                }
            };
            let sampled = stored_sampler.as_ref().map(sampler_of_stored);
            let blocks = self.block_sources
                && runtime
                    .device()
                    .features()
                    .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
            let uploaded = match texture_of(id, &stored, sampled, blocks) {
                Ok(texture) => {
                    let census = format!(
                        "stored texture {} ({}) uploaded as GraphicsFormat {}",
                        crate::textures::format_name(stored.format),
                        if stored.linear { "linear" } else { "sRGB" },
                        texture.graphics_format
                    );
                    let uploaded = self
                        .with_room(state, runtime, notes, |state, runtime| {
                            state.set_texture(runtime, &texture)
                        })
                        .map_err(|error| notes.fail("texture", &error))
                        .is_ok();
                    if uploaded {
                        *notes.census.entry(census).or_default() += 1;
                    }
                    uploaded
                }
                Err(error) => {
                    notes.fail("texture", &error);
                    notes.count(&format!(
                        "textureformat:{} {}x{}, {} bytes",
                        crate::textures::format_name(stored.format),
                        stored.width,
                        stored.height,
                        stored.bytes.len()
                    ));
                    false
                }
            };
            self.textures.insert(
                identity,
                Stored {
                    generation: stored.generation,
                    sampler_generation: stored_sampler.map_or(0, |found| found.generation),
                    usable: uploaded,
                    empty: false,
                    last_named: self.epoch,
                },
            );
        }
    }
}

/// The identities of the meshes the snapshot's renderers and mesh draws name.
fn named_meshes(snapshot: &GraphicsSnapshot) -> BTreeSet<u64> {
    let drawn = snapshot.work.iter().filter_map(|work| match work {
        OffscreenWork::DrawMesh { mesh, .. } => Some(*mesh),
        _ => None,
    });
    snapshot
        .renderers
        .iter()
        .map(|renderer| renderer.mesh)
        .chain(drawn)
        .collect()
}

fn usable(stored: &BTreeMap<u64, Stored>) -> BTreeSet<u64> {
    stored
        .iter()
        .filter(|(_, held)| held.usable)
        .map(|(identity, _)| *identity)
        .collect()
}

/// The identities of the stored textures the snapshot's properties and records name.
fn stored_textures(snapshot: &GraphicsSnapshot) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    let mut note = |texture: &TextureRef| {
        if let TextureRef::Asset(identity) = texture {
            found.insert(*identity);
        }
    };
    let properties = snapshot
        .materials
        .values()
        .map(|material| &material.properties)
        .chain(
            snapshot
                .renderers
                .iter()
                .filter_map(|r| r.property_block.as_ref()),
        )
        .chain(snapshot.global.iter().map(|global| &global.properties));
    for properties in properties {
        for (_, slot) in &properties.textures {
            note(&slot.texture);
        }
    }
    for work in &snapshot.work {
        match work {
            OffscreenWork::Blit { source, .. } | OffscreenWork::CopyTexture { source, .. } => {
                note(source);
            }
            OffscreenWork::Dispatch { bindings, .. } => {
                for (_, slot) in &bindings.properties.textures {
                    note(&slot.texture);
                }
            }
            OffscreenWork::SetGlobal(PropertyValue::Texture(_, slot)) => note(&slot.texture),
            _ => {}
        }
    }
    found
}

/// The mesh a blit draws: a quad over clip space as two triangles with no indices, whose texture
/// coordinates are the image's own (row 0 is clip y of -1).
pub(super) fn blit_mesh() -> Mesh {
    let corners: [(f32, f32); 6] = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (1.0, 1.0),
    ];
    let bytes =
        |values: Vec<f32>| -> Vec<u8> { values.into_iter().flat_map(f32::to_le_bytes).collect() };
    let positions = bytes(corners.iter().flat_map(|&(x, y)| [x, y, 0.0]).collect());
    let coordinates = bytes(
        corners
            .iter()
            .flat_map(|&(x, y)| [f32::midpoint(x, 1.0), f32::midpoint(y, 1.0)])
            .collect(),
    );
    Mesh {
        vertex_count: 6,
        channels: vec![
            MeshChannel {
                source: super::assets::POSITION,
                format: ChannelFormat::Float32x3,
                bytes: positions,
            },
            MeshChannel {
                source: super::assets::TEXTURE_COORDINATES,
                format: ChannelFormat::Float32x2,
                bytes: coordinates,
            },
        ],
        indices: MeshIndices::None,
        sub_meshes: vec![SubMesh {
            first: 0,
            count: 6,
            base_vertex: 0,
        }],
    }
}

/// Builds the frame's operations. When the frame as a whole is refused, the parts the state
/// refuses are left out and counted in the report by the name of their error: first each render
/// texture (with the records that name it), then each record on its own. What is left is built
/// together.
///
/// # Errors
/// Fails with the error of the frame that is left, which holds only parts that build alone, when
/// they still do not build together.
pub fn build(
    state: &mut OffscreenState,
    runtime: &mut ProgramRuntime,
    arena: &ProgramArena,
    adapted: &mut Adapted,
) -> Result<OffscreenOperations, OffscreenError> {
    let started = Instant::now();
    let operations = match state.build(runtime, arena, &adapted.frame) {
        Ok(operations) => operations,
        Err(first) => {
            let rebuilt = rebuild_without_unusable_parts(state, runtime, arena, adapted, first);
            adapted.report.phases.build += started.elapsed();
            rebuilt?
        }
    };
    let phases = &mut adapted.report.phases;
    if phases.refusals == 0 {
        phases.build += started.elapsed();
    }
    phases.programs += operations.program_time();
    phases.programs_built += operations.programs_built();
    let used = operations.constants_use();
    phases.constants_bytes += used.bytes;
    phases.constants_buffers += used.buffers;
    phases.constants_largest = phases.constants_largest.max(used.largest);
    phases.constants_blocks += used.blocks;
    phases.constants_shared += used.shared;
    adapted
        .report
        .count_many("draw:NoPassForCamera", operations.unmatched_draws());
    adapted.report.count_many(
        "draw:SampledWrittenLevelsOnly",
        operations.partial_mip_reads(),
    );
    adapted.report.count_many(
        "draw:MissingTexture",
        operations
            .missing_texture_draws()
            .iter()
            .map(|entry| entry.draws)
            .sum(),
    );
    adapted.report.count_many(
        "draw:MissingChannel",
        operations
            .missing_channel_draws()
            .iter()
            .map(|entry| entry.draws)
            .sum(),
    );
    adapted.report.absorb_draws(&operations);
    Ok(operations)
}

/// Builds the frame again after `first` refused it, without the parts that do not build alone.
///
/// The render textures come first, since the state creates them before it plans a record, so
/// `first` may be one of theirs. A refusal for the budget is retried once with the textures that
/// did not fit gone, and the records are tried alone only if that retry fails for another reason:
/// a record allocates no storage, so the budget is never its fault, and building each alone costs
/// as much as the frame.
fn rebuild_without_unusable_parts(
    state: &mut OffscreenState,
    runtime: &mut ProgramRuntime,
    arena: &ProgramArena,
    adapted: &mut Adapted,
    first: OffscreenError,
) -> Result<OffscreenOperations, OffscreenError> {
    adapted.report.phases.refusals += 1;
    if is_over_budget(&first) && adapted.report.holdings.is_empty() {
        adapted.report.holdings = holdings::describe(runtime);
    }
    // A refusal that is the whole frame's is not found in a part, and building the parts one at a
    // time costs as much as the frame for each of them.
    if first.is_frame_wide() {
        return Err(first);
    }
    adapted.report.phases.probes += adapted.frame.render_textures.len();
    drop_unusable_textures(state, runtime, arena, adapted);
    if is_over_budget(&first) {
        match state.build(runtime, arena, &adapted.frame) {
            Ok(operations) => return Ok(operations),
            Err(error) if is_over_budget(&error) => return Err(error),
            Err(_) => {}
        }
    }
    adapted.report.phases.probes += adapted.frame.records.len();
    drop_unusable_records(state, runtime, arena, adapted);
    state.build(runtime, arena, &adapted.frame)
}

fn is_over_budget(error: &OffscreenError) -> bool {
    matches!(
        error,
        OffscreenError::Program(ProgramError::OverBudget { .. })
    )
}

/// The frame with `records` only, and `textures` as its render textures.
fn part(
    frame: &OffscreenFrame,
    textures: &[RenderTextureSpec],
    records: Vec<Record>,
) -> OffscreenFrame {
    OffscreenFrame {
        render_textures: textures.to_vec(),
        records,
        later_reads: Vec::new(),
        ..frame.clone()
    }
}

/// Removes the render textures that do not build on their own and the records that name them.
fn drop_unusable_textures(
    state: &mut OffscreenState,
    runtime: &mut ProgramRuntime,
    arena: &ProgramArena,
    adapted: &mut Adapted,
) {
    let mut unusable = BTreeSet::new();
    for spec in &adapted.frame.render_textures {
        let alone = part(&adapted.frame, std::slice::from_ref(spec), Vec::new());
        if let Err(error) = state.build(runtime, arena, &alone) {
            adapted.report.fail("rendertexture", &error);
            unusable.insert(spec.id);
        }
    }
    if unusable.is_empty() {
        return;
    }
    adapted
        .frame
        .render_textures
        .retain(|spec| !unusable.contains(&spec.id));
    let before = adapted.frame.records.len();
    adapted
        .frame
        .records
        .retain(|record| !textures_of(record).iter().any(|id| unusable.contains(id)));
    let dropped = before - adapted.frame.records.len();
    adapted.report.built = adapted.report.built.saturating_sub(dropped);
    if dropped > 0 {
        *adapted
            .report
            .failures
            .entry("record:UnusableRenderTexture".to_owned())
            .or_default() += dropped;
    }
    adapted
        .frame
        .later_reads
        .retain(|read| !unusable.contains(&read.texture));
}

/// Removes the records that do not build on their own.
fn drop_unusable_records(
    state: &mut OffscreenState,
    runtime: &mut ProgramRuntime,
    arena: &ProgramArena,
    adapted: &mut Adapted,
) {
    let records = std::mem::take(&mut adapted.frame.records);
    let mut kept = Vec::new();
    // A record stands alone with the globals that were in force for it.
    let mut globals = None;
    for record in records {
        if matches!(record, Record::Globals(_)) {
            globals = Some(record.clone());
            kept.push(record);
            continue;
        }
        let alone = part(
            &adapted.frame,
            &adapted.frame.render_textures,
            globals.iter().cloned().chain([record.clone()]).collect(),
        );
        match state.build(runtime, arena, &alone) {
            Ok(_) => kept.push(record),
            Err(error) => {
                adapted.report.fail("record", &error);
                adapted.report.built = adapted.report.built.saturating_sub(1);
            }
        }
    }
    adapted.frame.records = kept;
}
