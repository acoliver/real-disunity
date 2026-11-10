//! Maps a snapshot of the world's graphics state onto render-program's [`OffscreenFrame`].
//!
//! The records keep the world's call order. A record that names something that no longer exists,
//! or that the host cannot supply, is left out and counted by the reason. A blit with every pass
//! becomes one record for each pass of its material's shader, in pass order.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::{CameraId, MaterialId, MeshId, RendererId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, RenderDimension, RenderTextureSpec, Renderer,
};
use worldrunner_render_program::unity::offscreen::{CameraProbe, StateVariant};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::ComponentHandle;
use worldrunner_world_simulation::graphics::{
    ComputeBindings, MaterialId as WorldMaterialId, OffscreenWork, RenderTextureId,
    ShaderId as WorldShaderId, TextureRef, WorldRenderTexture, dimension,
};
use worldrunner_world_simulation::presentation::CameraClear;

use super::bounds::Bounds;
use super::builtins::{OBJECT_TO_WORLD, VIEW_PROJECTION, camera_layer, draw_layer};
use super::defaults::{DefaultTexture, ShaderFill};
use super::ids;
use super::pixels::{self, Wanted};
use super::properties::{Textures, fallback_layer, layer, layer_with_defaults};
use super::script::{Script, kind};

pub use super::report::{Reclaimed, Report};
use worldrunner_world_simulation::graphics::{CameraView, GraphicsSnapshot, RendererView};

/// The tag key whose value names a pass's purpose.
pub(super) const PASS_TAG: &str = "LightMode";

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

/// What a test render clears its scratch target to: values a render is unlikely to write and a
/// half float, an unsigned 8-bit and a 32-bit float all hold to within a step.
pub const TEST_CLEAR: [f32; 4] = [0.3125, 0.6875, 0.4375, 0.8125];

/// The changed test renders, in the order of their numbers in [`ids::probe_variant`]: what each
/// changes in the state the passes declare, to find what leaves a render with no written texel.
/// The first three change one part of the state each, the fourth replaces the fragment stage, and
/// the last two combine them.
pub const TEST_VARIANTS: [(&str, StateVariant); 6] = [
    (
        "cull None",
        StateVariant {
            cull_none: true,
            blend_off: false,
            depth_always: false,
            constant_fragment: false,
        },
    ),
    (
        "blend off",
        StateVariant {
            cull_none: false,
            blend_off: true,
            depth_always: false,
            constant_fragment: false,
        },
    ),
    (
        "depth test Always",
        StateVariant {
            cull_none: false,
            blend_off: false,
            depth_always: true,
            constant_fragment: false,
        },
    ),
    (
        "a fragment stage that writes a constant",
        StateVariant {
            cull_none: false,
            blend_off: false,
            depth_always: false,
            constant_fragment: true,
        },
    ),
    (
        "cull None, blend off and depth test Always",
        StateVariant {
            cull_none: true,
            blend_off: true,
            depth_always: true,
            constant_fragment: false,
        },
    ),
    (
        "cull None, blend off, depth test Always and a constant fragment stage",
        StateVariant {
            cull_none: true,
            blend_off: true,
            depth_always: true,
            constant_fragment: true,
        },
    ),
];

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
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderInfo {
    /// The passes of its first sub-shader.
    pub passes: usize,
    /// The queue a material that names none takes.
    pub queue: i32,
    /// The `LightMode` values of the passes of its first sub-shader, in pass order. A pass with
    /// no such tag adds none.
    pub light_modes: Vec<String>,
    /// The texture properties whose declared default the host supplies. A material that sets no
    /// texture for one reads that default.
    pub texture_defaults: Vec<(String, DefaultTexture)>,
    /// The properties it declares and their defaults, which a material that sets none reads.
    pub fill: ShaderFill,
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
    /// The box of each uploaded mesh's vertices, which a culling plane and a shadow split's sphere
    /// are tested against.
    pub bounds: BTreeMap<u64, Bounds>,
    /// The index count of each uploaded mesh's sub-meshes, by range number, which a mesh draw
    /// checks its sub-mesh against.
    pub sub_meshes: BTreeMap<u64, Vec<u32>>,
    /// The meshes the host read and found to have nothing to draw. They are not in `meshes`.
    pub empty_meshes: BTreeSet<u64>,
    /// Whether a probed camera render also gets the changed test renders of [`TEST_VARIANTS`],
    /// each into a scratch copy of its own.
    pub test_variants: bool,
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
    /// A draw of a renderer list that no earlier record made, or that reads a culling result no
    /// earlier record made.
    UnknownList,
    /// A clear or a draw with no render target bound, or a target that names the camera's own
    /// when the camera has none.
    UnboundTarget,
    /// A target or a release that names a temporary render texture no allocation made.
    UnknownTemporary,
    /// A target that names a cube face or a slice past the first, which a render texture here
    /// does not have.
    UnsupportedSlice,
    /// A target that names a mip level its texture lacks.
    NoSuchMip,
    /// A mesh draw of a mesh the host's store does not hold.
    MeshUnavailable,
    /// A mesh draw of a sub-mesh number past the mesh's sub-meshes.
    NoSuchSubmesh,
    /// A mesh draw of a pass number past the shader's passes.
    NoSuchPass,
    /// A mesh draw of a mesh with nothing to draw, or of a sub-mesh with no indices.
    EmptyMesh,
}

/// Why a renderer was left out of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RendererSkip {
    MeshUnavailable,
    NoMaterial,
    EmptySlot,
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

/// Where a dispatch or a pixel read sits among the frame's records.
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
    /// The dispatches and pixel reads of the snapshot, in call order. The frame holds none: a
    /// host that runs them splits `frame.records` at these places.
    pub placements: Vec<Placed>,
}

/// The shape the world's description of a render texture states. A code the world does not
/// define is carried as unknown, for the state to refuse by name.
pub(super) fn render_dimension(texture: &WorldRenderTexture) -> RenderDimension {
    let depth = texture.volume_depth;
    match texture.dimension {
        dimension::TEX_2D => RenderDimension::D2,
        dimension::TEX_2D_ARRAY => RenderDimension::D2Array(depth),
        dimension::TEX_3D => RenderDimension::D3(depth),
        dimension::CUBE => RenderDimension::Cube,
        dimension::CUBE_ARRAY => RenderDimension::CubeArray(depth / 6),
        other => RenderDimension::Unknown(other),
    }
}

/// A render texture's side as a float. Sides above 65535 saturate, and none reaches them.
fn texels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}

/// Whether a camera's normalized rectangle is the whole target.
pub(super) fn fills_target(viewport: [f32; 4]) -> bool {
    viewport
        .iter()
        .zip([0.0, 0.0, 1.0, 1.0])
        .all(|(found, whole)| (found - whole).abs() <= f32::EPSILON)
}

/// Whether `record` is a test render, which draws into one of the frame's test targets and is not
/// among the records the world listed.
pub(super) fn is_test_render(
    record: &Record,
    test_targets: &BTreeSet<worldrunner_render_program::ids::TextureId>,
) -> bool {
    matches!(record, Record::CameraRender { target, .. } if test_targets.contains(target))
}

/// Whether `record` draws into a scratch copy, as every test render does.
fn is_scratch_render(record: &Record) -> bool {
    matches!(record, Record::CameraRender { target, .. } if ids::is_probe_scratch(*target))
}

/// The textures a record reads or writes.
pub(super) fn textures_of(record: &Record) -> Vec<worldrunner_render_program::ids::TextureId> {
    match *record {
        Record::CameraRender { target, .. } => vec![target],
        Record::Blit { source, target, .. } | Record::CopyTexture { source, target, .. } => {
            vec![source, target]
        }
        Record::Clear { ref target, .. } => target.textures().collect(),
        Record::Draws(ref list) => list.target.textures().collect(),
        Record::Globals(_) => Vec::new(),
    }
}

/// Maps `snapshot`.
#[must_use]
pub fn adapt(snapshot: &GraphicsSnapshot, available: &Available, settings: &Settings) -> Adapted {
    adapt_probing(snapshot, available, settings, false)
}

/// Maps `snapshot`. With `probe`, the camera renders into the render textures a dispatch binds are
/// examined: each is looked at on the CPU ([`CameraProbe`]) and drawn again into a scratch copy
/// cleared to [`TEST_CLEAR`], so the texels the camera wrote show as differing from it.
#[must_use]
pub fn adapt_probing(
    snapshot: &GraphicsSnapshot,
    available: &Available,
    settings: &Settings,
    probe: bool,
) -> Adapted {
    let mut mapper = Mapper::new(snapshot, available, settings);
    if probe {
        mapper.probe_dispatch_targets();
    }
    mapper.records();
    mapper.renderers();
    mapper.finish()
}

pub(super) struct Mapper<'a> {
    pub(super) snapshot: &'a GraphicsSnapshot,
    pub(super) available: &'a Available,
    pub(super) settings: &'a Settings,
    pub(super) textures: Textures<'a>,
    pub(super) frame: OffscreenFrame,
    pub(super) report: Report,
    /// What the frame's commands have set so far.
    pub(super) script: Script,
    placements: Vec<Placed>,
    cameras: BTreeMap<(ComponentHandle, [u32; 2]), CameraId>,
    queued: BTreeMap<(WorldMaterialId, i32), MaterialId>,
    /// The render textures whose camera renders are examined, and those with a scratch copy.
    probed: BTreeSet<RenderTextureId>,
    scratch: BTreeSet<RenderTextureId>,
    /// How many changed test renders each scratch copy has.
    variants: usize,
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
        let script = Script::new(frame.globals.clone(), &frame.global_keywords);
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
            script,
            placements: Vec::new(),
            cameras: BTreeMap::new(),
            queued: BTreeMap::new(),
            probed: BTreeSet::new(),
            scratch: BTreeSet::new(),
            variants: 0,
        }
    }

    /// Asks for a look at every camera render into a render texture some dispatch binds.
    fn probe_dispatch_targets(&mut self) {
        for work in &self.snapshot.work {
            let OffscreenWork::Dispatch { bindings, .. } = work else {
                continue;
            };
            for (_, slot) in &bindings.properties.textures {
                if let TextureRef::Render(id) = slot.texture {
                    self.probed.insert(id);
                }
            }
        }
        self.frame.probe = CameraProbe {
            targets: self
                .probed
                .iter()
                .map(|id| ids::render_texture(*id))
                .collect(),
            test_targets: BTreeSet::new(),
            object_to_world: OBJECT_TO_WORLD.to_owned(),
            view_projection: VIEW_PROJECTION.to_owned(),
            variants: BTreeMap::new(),
        };
    }

    fn records(&mut self) {
        for (index, work) in self.snapshot.work.iter().enumerate() {
            if let OffscreenWork::Dispatch { bindings, .. } = work {
                self.dispatch(index, bindings);
                continue;
            }
            if let Some(wanted) = Wanted::of(work) {
                self.pixel_read(index, wanted);
                continue;
            }
            match self.record(work) {
                Ok(records) => {
                    *self
                        .report
                        .built_by_kind
                        .entry(kind(work).to_owned())
                        .or_default() += 1;
                    self.report.built += records
                        .iter()
                        .filter(|record| !is_scratch_render(record))
                        .count();
                    if !records.is_empty() {
                        let globals = self.pending_globals();
                        self.frame.records.extend(globals);
                    }
                    self.frame.records.extend(records);
                }
                Err(skip) => {
                    *self.report.skipped.entry(skip).or_default() += 1;
                    *self
                        .report
                        .skipped_by_kind
                        .entry(format!("{}: {skip:?}", kind(work)))
                        .or_default() += 1;
                }
            }
        }
    }

    /// Notes where a dispatch sits and brings the render textures its properties name into the
    /// frame, so they exist when the dispatch binds them.
    fn dispatch(&mut self, work: usize, bindings: &ComputeBindings) {
        self.report.dispatches += 1;
        self.placements.push(Placed {
            after: self.frame.records.len(),
            work,
        });
        for (_, slot) in &bindings.properties.textures {
            self.textures.resolve(&slot.texture);
        }
    }

    /// Notes where a pixel read sits and brings its render texture into the frame, so it exists
    /// when the read is made. A texture of integers or one the snapshot lacks is refused here.
    fn pixel_read(&mut self, work: usize, wanted: Wanted) {
        let refusal = match self.snapshot.render_textures.get(&wanted.texture) {
            None => Some(pixels::REFUSED_NO_TEXTURE),
            Some(texture) if pixels::is_integer_format(texture.format) => {
                Some(pixels::REFUSED_INTEGER)
            }
            Some(_) => None,
        };
        if let Some(reason) = refusal {
            *self
                .report
                .pixel_reads_refused
                .entry(reason.to_owned())
                .or_default() += 1;
            return;
        }
        self.textures.resolve(&TextureRef::Render(wanted.texture));
        self.placements.push(Placed {
            after: self.frame.records.len(),
            work,
        });
    }

    fn record(&mut self, work: &OffscreenWork) -> Result<Vec<Record>, Skip> {
        match work {
            OffscreenWork::Dispatch { .. } => unreachable!("dispatches are placed, not mapped"),
            OffscreenWork::CameraRender { camera, target } => {
                let mut records = vec![self.camera_render(*camera, *target)?];
                if self.probed.contains(target) {
                    records.extend(self.test_renders(*camera, *target)?);
                }
                Ok(records)
            }
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
            OffscreenWork::ReadPixels { .. } => unreachable!("pixel reads are placed, not mapped"),
            _ => self.command(work),
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
            let id = CameraId(self.frame.cameras.len() as u64);
            self.frame.cameras.insert(id, self.camera(view, size));
            self.cameras.insert((handle, size), id);
            id
        };
        Ok(Record::CameraRender { camera: id, target })
    }

    /// The camera's render again into a scratch copy of `target`, cleared to [`TEST_CLEAR`], whose
    /// texels differ from the clear only where a fragment was written; and, when the caller asks
    /// for them, the same render made as each of [`TEST_VARIANTS`] says into a scratch copy of its
    /// own.
    fn test_renders(
        &mut self,
        handle: ComponentHandle,
        target: RenderTextureId,
    ) -> Result<Vec<Record>, Skip> {
        let view = self
            .snapshot
            .cameras
            .get(&handle)
            .ok_or(Skip::StaleCamera)?;
        let texture = self
            .snapshot
            .render_textures
            .get(&target)
            .ok_or(Skip::StaleTarget)?;
        let mut camera = self.camera(view, [texture.width, texture.height]);
        camera.clear_color = Some(TEST_CLEAR.map(f64::from));
        self.scratch.insert(target);
        let variants = if self.available.test_variants {
            TEST_VARIANTS.len()
        } else {
            0
        };
        self.variants = self.variants.max(variants);
        let base = ids::render_texture(target);
        let mut targets = vec![ids::probe_scratch(base)];
        for (number, (_, variant)) in (0..).zip(TEST_VARIANTS.iter().take(variants)) {
            let scratch = ids::probe_variant(base, number);
            self.frame.probe.variants.insert(scratch, *variant);
            targets.push(scratch);
        }
        Ok(targets
            .into_iter()
            .map(|scratch| {
                let id = CameraId(self.frame.cameras.len() as u64);
                self.frame.cameras.insert(id, camera.clone());
                Record::CameraRender {
                    camera: id,
                    target: scratch,
                }
            })
            .collect())
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
        let (id, passes) = self.material_and_passes(material)?;
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

    /// The frame's material for a world material that a blit or a mesh draw names, and how many
    /// passes its shader has.
    pub(super) fn material_and_passes(
        &mut self,
        id: WorldMaterialId,
    ) -> Result<(MaterialId, usize), Skip> {
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
    pub(super) fn material(
        &mut self,
        id: WorldMaterialId,
        queue: Option<i32>,
    ) -> Result<MaterialId, Skip> {
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
            let fallback = fallback_layer(&material.properties, &info.fill, &mut self.textures);
            self.frame.materials.insert(
                named,
                Material {
                    shader: Some(ids::shader(material.shader)),
                    keywords: material.keywords.clone(),
                    properties,
                    fallback,
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
            if self.script.renderers.contains_key(&index) {
                continue;
            }
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

    pub(super) fn renderer(&mut self, view: &RendererView) -> Result<Renderer, RendererSkip> {
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

    /// Notes that the frame holds a render texture of `code`.
    fn note_format(&mut self, code: u32) {
        self.report.formats.insert(code);
        *self
            .report
            .census
            .entry(format!("frame render texture of format {code}"))
            .or_default() += 1;
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
        let temporaries: BTreeSet<_> = self
            .frame
            .records
            .iter()
            .flat_map(textures_of)
            .filter(|id| ids::is_temporary(*id))
            .collect();
        for spec in self.temporary_specs(&temporaries) {
            self.note_format(spec.graphics_format);
            self.frame.render_textures.push(spec);
        }
        for id in used {
            let texture = &self.snapshot.render_textures[&id];
            self.note_format(texture.format);
            self.frame.render_textures.push(RenderTextureSpec {
                id: ids::render_texture(id),
                size: [texture.width, texture.height],
                dimension: render_dimension(texture),
                graphics_format: texture.format,
                depth_bits: u8::try_from(texture.depth_bits).unwrap_or(u8::MAX),
                mips: texture.mip_count,
                srgb: texture.srgb,
                generation: texture.generation,
            });
        }
        // A scratch copy is made like the render texture it copies.
        let copies: Vec<_> = self
            .frame
            .render_textures
            .iter()
            .filter(|spec| {
                ids::render_texture_of(spec.id).is_some_and(|id| self.scratch.contains(&id))
            })
            .flat_map(|spec| {
                let variants = (0..)
                    .take(self.variants)
                    .map(|number| ids::probe_variant(spec.id, number));
                std::iter::once(ids::probe_scratch(spec.id))
                    .chain(variants)
                    .map(|id| RenderTextureSpec { id, ..spec.clone() })
                    .collect::<Vec<_>>()
            })
            .collect();
        self.frame.render_textures.extend(copies);
        let variants = self.variants;
        self.frame.probe.test_targets = self
            .scratch
            .iter()
            .flat_map(|id| {
                let base = ids::render_texture(*id);
                std::iter::once(ids::probe_scratch(base)).chain(
                    (0..)
                        .take(variants)
                        .map(move |number| ids::probe_variant(base, number)),
                )
            })
            .collect();
        self.report.unresolved_textures = self.textures.unresolved().clone();
        for (rule, count) in self.textures.filled() {
            *self
                .report
                .census
                .entry(format!("constants filled by {rule}"))
                .or_default() += count;
        }
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
            placements: self.placements,
        }
    }
}
