//! Maps a snapshot of the world's graphics state onto render-program's [`OffscreenFrame`].
//!
//! The records keep the world's call order. A record that names something that no longer exists,
//! or that the host cannot supply, is left out and counted by the reason. A blit with every pass
//! becomes one record for each pass of its material's shader, in pass order.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Display};

use worldrunner_render_program::ids::{CameraId, MaterialId, MeshId, RendererId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, RenderTextureSpec, Renderer,
};
use worldrunner_render_program::unity::offscreen::{
    CameraDraws, OffscreenOperations, PartialMipReads, UnmatchedPasses,
};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::ComponentHandle;
use worldrunner_world_simulation::graphics::{
    ComputeBindings, MaterialId as WorldMaterialId, OffscreenWork, PropertyValue as WorldValue,
    RenderTextureId, ShaderId as WorldShaderId, TextureRef,
};
use worldrunner_world_simulation::presentation::CameraClear;

use super::builtins::{camera_layer, draw_layer};
use super::compute::DispatchTrace;
use super::defaults::DefaultTexture;
use super::ids;
use super::properties::{Textures, layer, layer_with_defaults};
use super::trace::Phases;
use worldrunner_world_simulation::graphics::{CameraView, GraphicsSnapshot, RendererView};

/// The tag key whose value names a pass's purpose.
const PASS_TAG: &str = "LightMode";

/// The `LightMode` values a camera draws, in the order of preference.
const CAMERA_LIGHT_MODES: [&str; 5] = [
    "UniversalForward",
    "UniversalForwardOnly",
    "SRPDefaultUnlit",
    "ForwardBase",
    "Always",
];

/// The mode a pass with no `LightMode` counts as, the last of [`CAMERA_LIGHT_MODES`].
const UNTAGGED_LIGHT_MODE: &str = "Always";

/// The mesh a blit draws.
pub const BLIT_MESH: MeshId = MeshId(u64::MAX);

/// What the caller chooses.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// The passes a camera draws: the first of these `LightMode` values that any pass of a
    /// shader has selects the pass.
    pub light_modes: Vec<String>,
    /// The `LightMode` a pass with no such tag counts as having, if it counts as one.
    pub untagged_light_mode: Option<String>,
}

impl Settings {
    /// What the player's cameras draw. A pass with no `LightMode` counts as `Always`, the last of
    /// the modes, so a pass tagged with any other selected mode wins over it and a pass tagged
    /// with a mode that is not selected is never drawn.
    #[must_use]
    pub fn cameras() -> Self {
        Self {
            light_modes: CAMERA_LIGHT_MODES.map(str::to_owned).to_vec(),
            untagged_light_mode: Some(UNTAGGED_LIGHT_MODE.to_owned()),
        }
    }

    /// The passes a camera draws.
    #[must_use]
    pub fn pass_selector(&self) -> PassSelector {
        let (key, values) = (PASS_TAG.to_owned(), self.light_modes.clone());
        match &self.untagged_light_mode {
            Some(untagged) => PassSelector::TagWithUntagged {
                key,
                values,
                untagged: untagged.clone(),
            },
            None => PassSelector::Tag { key, values },
        }
    }
}

/// What the shader of a material offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderInfo {
    /// The passes of its first sub-shader.
    pub passes: usize,
    /// The queue a material that names none takes.
    pub queue: i32,
    /// The texture properties whose declared default the host supplies. A material that sets no
    /// texture for one reads that default.
    pub texture_defaults: Vec<(String, DefaultTexture)>,
}

/// What the host can supply for the snapshot: the shaders it loaded, and the identities of the
/// meshes and stored textures it uploaded.
#[derive(Clone, Debug, Default)]
pub struct Available {
    pub shaders: BTreeMap<WorldShaderId, ShaderInfo>,
    pub meshes: BTreeSet<u64>,
    pub textures: BTreeSet<u64>,
    /// The default textures that are registered and can be named.
    pub default_textures: BTreeSet<DefaultTexture>,
}

/// Why a record was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Skip {
    StaleCamera,
    StaleTarget,
    StaleSource,
    StaleMaterial,
    ShaderUnavailable,
    SourceUnavailable,
    NoBlitSource,
    PartialViewport,
    /// A copy or blit source that names a global texture, which a record never does.
    GlobalSource,
    /// A script command the adapter does not run: a render target, clear, global, temporary
    /// texture, camera setup or culling record.
    UnsupportedWork,
}

/// Why a renderer was left out of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RendererSkip {
    MeshUnavailable,
    NoMaterial,
    EmptySlot,
}

/// What the mapping did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The records the world listed.
    pub records: usize,
    /// The records of the frame, after a blit of every pass became one for each pass.
    pub built: usize,
    /// The dispatches among the records. They are not frame records; see [`Placed`].
    pub dispatches: usize,
    pub skipped: BTreeMap<Skip, usize>,
    pub renderers_skipped: BTreeMap<RendererSkip, usize>,
    /// The `GraphicsFormat` codes of the render textures the frame uses.
    pub formats: BTreeSet<u32>,
    /// What could not be loaded, uploaded or built, by the kind of thing and the name of the
    /// error, such as `shader:NoVulkanPlatform`.
    pub failures: BTreeMap<String, usize>,
    /// The same failures by the chain of variant names inside the error, such as
    /// `texture:Program/Layout/UnsupportedParameter`, and the first message of each chain.
    pub causes: BTreeMap<String, usize>,
    pub examples: BTreeMap<String, String>,
    /// How each dispatch was bound and what came of it, in call order.
    pub traces: Vec<DispatchTrace>,
    /// What each camera render recorded and left out.
    pub camera_draws: Vec<CameraDraws>,
    /// The camera draws left out for want of a pass, and the texture reads that bound only the
    /// levels written with the base level, each counted by the tags of the pass concerned.
    pub unmatched_passes: Vec<UnmatchedPasses>,
    pub partial_reads: Vec<PartialMipReads>,
    /// The camera draws left out because no layer names a texture the pass samples, by the
    /// texture property.
    pub missing_textures: BTreeMap<String, usize>,
    /// The camera draws left out because the mesh lacks a vertex channel the pass reads, by
    /// `channel C for input I`.
    pub missing_channels: BTreeMap<String, usize>,
    /// The texture properties that named no texture the frame can read, by `name: reason`.
    pub unresolved_textures: BTreeMap<String, usize>,
    /// Where building the frame spent its time.
    pub phases: Phases,
    /// The script commands the adapter does not run, by what they are: `SetGlobal texture _Name
    /// (render)`, `SetRenderTarget`, and so on.
    pub unsupported_work: BTreeMap<String, usize>,
    /// The textures the world's globals hold when the frame is mapped, by `name: kind`.
    pub global_textures: BTreeMap<String, usize>,
}

impl Report {
    /// Takes what `operations` counted of the draws it holds.
    pub fn absorb_draws(&mut self, operations: &OffscreenOperations) {
        self.camera_draws.extend(operations.camera_draws());
        for entry in operations.unmatched_passes() {
            self.add_unmatched(entry);
        }
        for entry in operations.partial_mip_reads_by_pass() {
            self.add_partial(entry);
        }
        for entry in operations.missing_texture_draws() {
            *self.missing_textures.entry(entry.texture).or_default() += entry.draws;
        }
        for entry in operations.missing_channel_draws() {
            let name = format!("channel {} for input {}", entry.channel, entry.input);
            *self.missing_channels.entry(name).or_default() += entry.draws;
        }
    }

    fn add_unmatched(&mut self, entry: UnmatchedPasses) {
        let same = |held: &&mut UnmatchedPasses| {
            (&held.key, &held.selected, &held.tags, held.passes)
                == (&entry.key, &entry.selected, &entry.tags, entry.passes)
        };
        if let Some(held) = self.unmatched_passes.iter_mut().find(|held| same(held)) {
            held.draws += entry.draws;
        } else {
            self.unmatched_passes.push(entry);
        }
    }

    fn add_partial(&mut self, entry: PartialMipReads) {
        let same = |held: &&mut PartialMipReads| {
            (
                &held.texture,
                held.written,
                held.total,
                &held.key,
                &held.tag,
            ) == (
                &entry.texture,
                entry.written,
                entry.total,
                &entry.key,
                &entry.tag,
            )
        };
        if let Some(held) = self.partial_reads.iter_mut().find(|held| same(held)) {
            held.reads += entry.reads;
        } else {
            self.partial_reads.push(entry);
        }
    }

    /// Counts a failure of `kind` by the name of `error` and by the chain of names inside it.
    pub fn fail(&mut self, kind: &str, error: &(impl Debug + Display)) {
        let name = format!("{kind}:{}", variant_name(error));
        *self.failures.entry(name).or_default() += 1;
        let cause = format!("{kind}:{}", variant_chain(error));
        *self.causes.entry(cause.clone()).or_default() += 1;
        self.examples
            .entry(cause)
            .or_insert_with(|| error.to_string());
    }

    /// Counts a failure that has no error behind it, by the whole of its name.
    pub fn count(&mut self, name: &str) {
        self.count_many(name, 1);
    }

    /// Counts `times` failures of the same name. Nothing is counted for zero.
    pub fn count_many(&mut self, name: &str, times: usize) {
        if times > 0 {
            *self.failures.entry(name.to_owned()).or_default() += times;
            *self.causes.entry(name.to_owned()).or_default() += times;
        }
    }

    /// Adds the failures, causes and examples of `other` to this report's, keeping the first
    /// example of each cause.
    pub fn absorb_failures(&mut self, other: Self) {
        for (name, count) in other.failures {
            *self.failures.entry(name).or_default() += count;
        }
        for (cause, count) in other.causes {
            *self.causes.entry(cause).or_default() += count;
        }
        for (cause, example) in other.examples {
            self.examples.entry(cause).or_insert(example);
        }
        for (name, count) in other.unresolved_textures {
            *self.unresolved_textures.entry(name).or_default() += count;
        }
        self.camera_draws.extend(other.camera_draws);
        self.phases.add(&other.phases);
        for (name, count) in other.unsupported_work {
            *self.unsupported_work.entry(name).or_default() += count;
        }
        for (name, count) in other.global_textures {
            *self.global_textures.entry(name).or_default() += count;
        }
        for entry in other.unmatched_passes {
            self.add_unmatched(entry);
        }
        for entry in other.partial_reads {
            self.add_partial(entry);
        }
        for (name, count) in other.missing_textures {
            *self.missing_textures.entry(name).or_default() += count;
        }
        for (name, count) in other.missing_channels {
            *self.missing_channels.entry(name).or_default() += count;
        }
    }
}

/// The leading identifier of an error's `Debug` form: the name of its variant.
pub(super) fn variant_name(error: &dyn Debug) -> String {
    variant_chain(error)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The names of the variants nested in an error's `Debug` form, outermost first and joined by
/// `/`: `Program(Layout(UnsupportedParameter { kind: ".." }))` is `Program/Layout/UnsupportedParameter`.
/// The chain ends at a variant whose payload is not another variant.
pub(super) fn variant_chain(error: &dyn Debug) -> String {
    let text = format!("{error:?}");
    let mut names = Vec::new();
    let mut rest = text.as_str();
    loop {
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        names.push(&rest[..end]);
        match rest[end..].strip_prefix('(') {
            Some(inner) => rest = inner,
            None => break,
        }
    }
    names.join("/")
}

/// What kind of texture a reference names, for reports.
fn texture_kind(texture: &TextureRef) -> &'static str {
    match texture {
        TextureRef::None => "none",
        TextureRef::Global(_) => "global",
        TextureRef::Render(_) => "render",
        TextureRef::Asset(_) => "asset",
    }
}

/// A script command the adapter does not run, named by what it is.
fn describe(work: &OffscreenWork) -> String {
    match work {
        OffscreenWork::SetGlobal(WorldValue::Texture(name, slot)) => {
            format!("SetGlobal texture {name} ({})", texture_kind(&slot.texture))
        }
        OffscreenWork::SetGlobal(_) => "SetGlobal value".to_owned(),
        OffscreenWork::SetGlobalKeyword { .. } => "SetGlobalKeyword".to_owned(),
        OffscreenWork::SetRenderTarget { .. } => "SetRenderTarget".to_owned(),
        OffscreenWork::ClearRenderTarget { .. } => "ClearRenderTarget".to_owned(),
        OffscreenWork::AllocateTemporary { .. } => "AllocateTemporary".to_owned(),
        OffscreenWork::ReleaseTemporary { .. } => "ReleaseTemporary".to_owned(),
        OffscreenWork::SetupCamera { .. } => "SetupCamera".to_owned(),
        OffscreenWork::Cull { .. } => "Cull".to_owned(),
        _ => "work the adapter maps".to_owned(),
    }
}

/// Where a dispatch sits among the frame's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// How many of the frame's records come before it.
    pub after: usize,
    /// Its index among the snapshot's work.
    pub work: usize,
}

#[derive(Debug)]
pub struct Adapted {
    pub frame: OffscreenFrame,
    pub report: Report,
    /// The dispatches of the snapshot, in call order. The frame holds none: a host that runs
    /// them splits `frame.records` at these places.
    pub dispatches: Vec<Placed>,
}

/// A render texture's side as a float. Sides above 65535 saturate, and none reaches them.
fn texels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}

/// Whether a camera's normalized rectangle is the whole target.
fn fills_target(viewport: [f32; 4]) -> bool {
    viewport
        .iter()
        .zip([0.0, 0.0, 1.0, 1.0])
        .all(|(found, whole)| (found - whole).abs() <= f32::EPSILON)
}

/// The textures a record reads or writes.
pub(super) fn textures_of(record: &Record) -> Vec<worldrunner_render_program::ids::TextureId> {
    match *record {
        Record::CameraRender { target, .. } => vec![target],
        Record::Blit { source, target, .. } | Record::CopyTexture { source, target, .. } => {
            vec![source, target]
        }
    }
}

/// Maps `snapshot`.
#[must_use]
pub fn adapt(snapshot: &GraphicsSnapshot, available: &Available, settings: &Settings) -> Adapted {
    let mut mapper = Mapper::new(snapshot, available, settings);
    mapper.records();
    mapper.renderers();
    mapper.finish()
}

struct Mapper<'a> {
    snapshot: &'a GraphicsSnapshot,
    available: &'a Available,
    settings: &'a Settings,
    textures: Textures<'a>,
    frame: OffscreenFrame,
    report: Report,
    dispatches: Vec<Placed>,
    cameras: BTreeMap<(ComponentHandle, [u32; 2]), CameraId>,
    queued: BTreeMap<(WorldMaterialId, i32), MaterialId>,
}

impl<'a> Mapper<'a> {
    fn new(
        snapshot: &'a GraphicsSnapshot,
        available: &'a Available,
        settings: &'a Settings,
    ) -> Self {
        let mut textures = Textures::new(&snapshot.render_textures, &available.textures);
        let mut frame = OffscreenFrame::default();
        if let Some(global) = &snapshot.global {
            frame.globals = layer(&global.properties, &mut textures);
            frame.global_keywords.clone_from(&global.keywords);
        }
        Self {
            snapshot,
            available,
            settings,
            textures,
            frame,
            report: Report {
                records: snapshot.work.len(),
                ..Report::default()
            },
            dispatches: Vec::new(),
            cameras: BTreeMap::new(),
            queued: BTreeMap::new(),
        }
    }

    fn records(&mut self) {
        for (index, work) in self.snapshot.work.iter().enumerate() {
            if let OffscreenWork::Dispatch { bindings, .. } = work {
                self.dispatch(index, bindings);
                continue;
            }
            match self.record(work) {
                Ok(records) => {
                    self.report.built += records.len();
                    self.frame.records.extend(records);
                }
                Err(skip) => {
                    *self.report.skipped.entry(skip).or_default() += 1;
                    if skip == Skip::UnsupportedWork {
                        *self
                            .report
                            .unsupported_work
                            .entry(describe(work))
                            .or_default() += 1;
                    }
                }
            }
        }
    }

    /// Notes where a dispatch sits and brings the render textures its properties name into the
    /// frame, so they exist when the dispatch binds them.
    fn dispatch(&mut self, work: usize, bindings: &ComputeBindings) {
        self.report.dispatches += 1;
        self.dispatches.push(Placed {
            after: self.frame.records.len(),
            work,
        });
        for (_, slot) in &bindings.properties.textures {
            self.textures.resolve(&slot.texture);
        }
    }

    fn record(&mut self, work: &OffscreenWork) -> Result<Vec<Record>, Skip> {
        match work {
            OffscreenWork::Dispatch { .. } => unreachable!("dispatches are placed, not mapped"),
            OffscreenWork::CameraRender { camera, target } => self
                .camera_render(*camera, *target)
                .map(|record| vec![record]),
            OffscreenWork::Blit {
                source,
                target,
                material,
                pass,
            } => self.blit(source, *target, *material, *pass),
            OffscreenWork::CopyTexture {
                source,
                source_element,
                source_mip,
                target,
                target_element,
                target_mip,
            } => {
                let (source, target) = (self.source(source)?, self.target(*target)?);
                Ok(vec![Record::CopyTexture {
                    source,
                    source_element: *source_element,
                    source_mip: *source_mip,
                    target,
                    target_element: *target_element,
                    target_mip: *target_mip,
                }])
            }
            _ => Err(Skip::UnsupportedWork),
        }
    }

    /// The name of a render texture a record writes.
    fn target(
        &mut self,
        target: RenderTextureId,
    ) -> Result<worldrunner_render_program::ids::TextureId, Skip> {
        if !self.snapshot.render_textures.contains_key(&target) {
            return Err(Skip::StaleTarget);
        }
        Ok(ids::render_texture(target))
    }

    /// The name of a texture a record reads.
    fn source(
        &mut self,
        source: &TextureRef,
    ) -> Result<worldrunner_render_program::ids::TextureId, Skip> {
        match *source {
            TextureRef::None => Err(Skip::NoBlitSource),
            TextureRef::Global(_) => Err(Skip::GlobalSource),
            TextureRef::Render(id) => {
                if !self.snapshot.render_textures.contains_key(&id) {
                    return Err(Skip::StaleSource);
                }
                Ok(ids::render_texture(id))
            }
            TextureRef::Asset(identity) => {
                if !self.available.textures.contains(&identity) {
                    return Err(Skip::SourceUnavailable);
                }
                ids::stored_texture(identity).ok_or(Skip::SourceUnavailable)
            }
        }
    }

    fn camera_render(
        &mut self,
        handle: ComponentHandle,
        target: RenderTextureId,
    ) -> Result<Record, Skip> {
        let view = self
            .snapshot
            .cameras
            .get(&handle)
            .ok_or(Skip::StaleCamera)?;
        let size = self
            .snapshot
            .render_textures
            .get(&target)
            .map(|texture| [texture.width, texture.height])
            .ok_or(Skip::StaleTarget)?;
        if !fills_target(view.settings.viewport) {
            return Err(Skip::PartialViewport);
        }
        let target = self.target(target)?;
        let id = if let Some(id) = self.cameras.get(&(handle, size)) {
            *id
        } else {
            let id = CameraId(self.cameras.len() as u64);
            self.frame.cameras.insert(id, self.camera(view, size));
            self.cameras.insert((handle, size), id);
            id
        };
        Ok(Record::CameraRender { camera: id, target })
    }

    /// What the camera clears, from its clear flags, and what it draws.
    fn camera(&self, view: &CameraView, size: [u32; 2]) -> Camera {
        let (clear_color, clear_depth) = match view.settings.clear {
            CameraClear::Skybox | CameraClear::SolidColor => (Some(view.settings.background), true),
            CameraClear::DepthOnly => (None, true),
            CameraClear::Nothing => (None, false),
        };
        Camera {
            culling_mask: view.settings.culling_mask,
            clear_color,
            clear_depth,
            pass: self.settings.pass_selector(),
            values: camera_layer(view, texels(size[0]) / texels(size[1])),
            // Unity winds front faces clockwise as seen with up up. The image is the other way up
            // (see `camera::clip_from_gl`), which reverses a winding.
            front_face: wgpu::FrontFace::Ccw,
        }
    }

    fn blit(
        &mut self,
        source: &TextureRef,
        target: RenderTextureId,
        material: Option<WorldMaterialId>,
        pass: i32,
    ) -> Result<Vec<Record>, Skip> {
        let source = self.source(source)?;
        let target = self.target(target)?;
        let Some(material) = material else {
            self.frame.blit_mesh.get_or_insert(BLIT_MESH);
            return Ok(vec![Record::Blit {
                source,
                target,
                material: None,
                pass: 0,
            }]);
        };
        let (id, passes) = self.blit_material(material)?;
        self.frame.blit_mesh = Some(BLIT_MESH);
        let passes = match usize::try_from(pass) {
            Ok(pass) => pass..pass + 1,
            Err(_) => 0..passes,
        };
        Ok(passes
            .map(|pass| Record::Blit {
                source,
                target,
                material: Some(id),
                pass,
            })
            .collect())
    }

    /// A blit's material, and how many passes its shader has.
    fn blit_material(&mut self, id: WorldMaterialId) -> Result<(MaterialId, usize), Skip> {
        let material = self
            .snapshot
            .materials
            .get(&id)
            .ok_or(Skip::StaleMaterial)?;
        let info = self
            .available
            .shaders
            .get(&material.shader)
            .ok_or(Skip::ShaderUnavailable)?;
        Ok((self.material(id, None)?, info.passes))
    }

    /// The frame's material for a world material, with `queue` in place of its own when the
    /// renderer has one.
    fn material(&mut self, id: WorldMaterialId, queue: Option<i32>) -> Result<MaterialId, Skip> {
        let material = self
            .snapshot
            .materials
            .get(&id)
            .ok_or(Skip::StaleMaterial)?;
        let info = self
            .available
            .shaders
            .get(&material.shader)
            .ok_or(Skip::ShaderUnavailable)?;
        let own = if material.render_queue < 0 {
            info.queue
        } else {
            material.render_queue
        };
        let (named, queue) = match queue {
            Some(queue) => {
                let next = u32::try_from(self.queued.len()).unwrap_or(u32::MAX);
                let named = *self
                    .queued
                    .entry((id, queue))
                    .or_insert_with(|| ids::queued_material(next));
                (named, queue)
            }
            None => (ids::material(id), own),
        };
        if !self.frame.materials.contains_key(&named) {
            let defaults: Vec<(String, TextureId)> = info
                .texture_defaults
                .iter()
                .filter(|(_, texture)| self.available.default_textures.contains(texture))
                .map(|(name, texture)| (name.clone(), texture.id()))
                .collect();
            let properties =
                layer_with_defaults(&material.properties, &mut self.textures, &defaults);
            self.frame.materials.insert(
                named,
                Material {
                    shader: Some(ids::shader(material.shader)),
                    keywords: material.keywords.clone(),
                    properties,
                    render_queue: queue,
                },
            );
        }
        Ok(named)
    }

    /// The renderers some camera of the frame can see.
    fn renderers(&mut self) {
        let mask = self
            .frame
            .cameras
            .values()
            .fold(0_u32, |mask, camera| mask | camera.culling_mask);
        for (index, view) in self.snapshot.renderers.iter().enumerate() {
            if mask
                .checked_shr(view.layer)
                .is_none_or(|bits| bits & 1 == 0)
            {
                continue;
            }
            match self.renderer(view) {
                Ok(renderer) => {
                    self.frame
                        .renderers
                        .insert(RendererId(index as u64), renderer);
                }
                Err(skip) => *self.report.renderers_skipped.entry(skip).or_default() += 1,
            }
        }
    }

    fn renderer(&mut self, view: &RendererView) -> Result<Renderer, RendererSkip> {
        if !self.available.meshes.contains(&view.mesh) {
            return Err(RendererSkip::MeshUnavailable);
        }
        // A slot with no material draws nothing, but the frame's slots are positional: a gap
        // before a drawn slot cannot be said, so that renderer is left out. Slots after the last
        // drawn one are simply not listed.
        let mut slots = Vec::new();
        let mut gap = false;
        for slot in &view.materials {
            match slot.and_then(|id| self.material(id, view.render_queue).ok()) {
                Some(_) if gap => return Err(RendererSkip::EmptySlot),
                Some(named) => slots.push(named),
                None => gap = true,
            }
        }
        if slots.is_empty() {
            return Err(RendererSkip::NoMaterial);
        }
        let block = view
            .property_block
            .as_ref()
            .map_or_else(PropertyLayer::default, |block| {
                layer(block, &mut self.textures)
            });
        Ok(Renderer {
            mesh: ids::mesh(view.mesh),
            materials: slots,
            layer: view.layer,
            block,
            per_draw: draw_layer(&view.local_to_world),
        })
    }

    fn finish(mut self) -> Adapted {
        let mut used = self.textures.used().clone();
        used.extend(
            self.frame
                .records
                .iter()
                .flat_map(textures_of)
                .filter_map(ids::render_texture_of),
        );
        for id in used {
            let texture = &self.snapshot.render_textures[&id];
            self.report.formats.insert(texture.format);
            self.frame.render_textures.push(RenderTextureSpec {
                id: ids::render_texture(id),
                size: [texture.width, texture.height],
                layers: 1,
                graphics_format: texture.format,
                depth_bits: u8::try_from(texture.depth_bits).unwrap_or(u8::MAX),
                mips: texture.mip_count,
                srgb: texture.srgb,
                generation: texture.generation,
            });
        }
        self.report.unresolved_textures = self.textures.unresolved().clone();
        if let Some(global) = &self.snapshot.global {
            for (name, slot) in &global.properties.textures {
                let kind = texture_kind(&slot.texture);
                *self
                    .report
                    .global_textures
                    .entry(format!("{name}: {kind}"))
                    .or_default() += 1;
            }
        }
        Adapted {
            frame: self.frame,
            report: self.report,
            dispatches: self.dispatches,
        }
    }
}
