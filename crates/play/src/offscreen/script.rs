//! The commands of a frame's command lists: what each sets in the host's running state and the
//! records it becomes.
//!
//! The world lists a frame's work in the order the script called it, and the host follows that
//! order with a running state: the render target the draws go to, the camera the script set up,
//! the global values and keywords, and the temporary render textures. A command that changes the
//! state becomes no record; a clear and a draw list become records that carry what they need, so
//! a record does not depend on the state at the time the frame is built. The global values are
//! the exception that costs least to keep as a record of their own: a [`Record::Globals`] stands
//! in front of the first record that follows a change.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use glam::Mat4;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::layers::TextureInput;
use worldrunner_render_program::unity::offscreen::frame::Attachment;
use worldrunner_render_program::unity::offscreen::frame::Attachments;
use worldrunner_render_program::unity::offscreen::frame::Clear;
use worldrunner_render_program::unity::offscreen::frame::GlobalState;
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_render_program::unity::offscreen::frame::RenderDimension;
use worldrunner_render_program::unity::offscreen::frame::RenderTextureSpec;
use worldrunner_world_simulation::ComponentHandle;
use worldrunner_world_simulation::graphics::CameraBuffers;
use worldrunner_world_simulation::graphics::MAX_COLOR_TARGETS;
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::graphics::PropertyValue as WorldValue;
use worldrunner_world_simulation::graphics::RendererListDesc;
use worldrunner_world_simulation::graphics::TargetRef;
use worldrunner_world_simulation::graphics::TextureRef;
use worldrunner_world_simulation::graphics::WorldRenderTexture;
use worldrunner_world_simulation::graphics::WorldSampler;
use worldrunner_world_simulation::graphics::dimension;

use super::assets::sampler_of_world;
use super::frame::Mapper;
use super::frame::RendererSkip;
use super::frame::Skip;
use super::ids;
use super::properties::set_value;
use super::raster::RasterScript;

/// The temporary render textures of a frame: the live one under each property name id, and every
/// allocation's description. An id is the name id and the number of allocations of that name before
/// it in the frame, so a name allocated again gets another texture.
#[derive(Default)]
pub(super) struct Temporaries {
    live: BTreeMap<i32, TextureId>,
    allocations: BTreeMap<i32, u32>,
    pub(super) described: BTreeMap<TextureId, WorldRenderTexture>,
}

impl Temporaries {
    fn allocate(&mut self, name_id: i32, desc: WorldRenderTexture) -> TextureId {
        let count = self.allocations.entry(name_id).or_default();
        let id = ids::temporary(name_id, *count);
        *count += 1;
        self.live.insert(name_id, id);
        self.described.insert(id, desc);
        id
    }

    fn release(&mut self, name_id: i32) -> bool {
        self.live.remove(&name_id).is_some()
    }

    pub(super) fn resolve(&self, name_id: i32) -> Option<TextureId> {
        self.live.get(&name_id).copied()
    }
}

/// The temporary render textures that `work` allocates, which the host keeps for the frame.
#[must_use]
pub fn temporary_ids(work: &[OffscreenWork]) -> BTreeSet<TextureId> {
    let mut temporaries = Temporaries::default();
    for command in work {
        if let OffscreenWork::AllocateTemporary { name_id, desc } = command {
            temporaries.allocate(*name_id, *desc);
        }
    }
    temporaries.described.into_keys().collect()
}

/// The `GraphicsFormat` of the screen's colour: four 8-bit channels stored sRGB.
const SCREEN_COLOR_FORMAT: u32 = 4;
/// The depth bits of the screen's depth buffer.
const SCREEN_DEPTH_BITS: u32 = 24;

/// The screen's texture as the frame describes it: a colour format with depth bits, as a camera's
/// render texture has. It holds its contents from frame to frame, as a render texture of the world
/// does, until the screen's size changes.
struct Screen {
    texture: WorldRenderTexture,
}

impl Screen {
    fn new([width, height]: [u32; 2]) -> Self {
        let texture = WorldRenderTexture {
            width,
            height,
            format: SCREEN_COLOR_FORMAT,
            depth_bits: SCREEN_DEPTH_BITS,
            mip_count: 1,
            auto_generate_mips: false,
            srgb: true,
            dimension: dimension::TEX_2D,
            volume_depth: 1,
            sampler: WorldSampler::default(),
            generation: 0,
        };
        Self { texture }
    }

    fn describe(&self, texture: TextureId) -> Option<&WorldRenderTexture> {
        (texture == ids::screen_color()).then_some(&self.texture)
    }
}

/// The camera a script set up: where its draws go when a target names the camera's own (its
/// target texture, or the buffers a script set; with neither, the screen of the host), the view
/// and the projection (column-major OpenGL matrices, as the world holds them) the draws use, and
/// the camera's rectangle of its target.
pub(super) struct CameraSetup {
    pub(super) target: TargetRef,
    /// The buffers the camera drew into when the script set it up, which are its target when it
    /// has them.
    pub(super) buffers: Option<CameraBuffers>,
    pub(super) view: Mat4,
    pub(super) projection: Mat4,
    pub(super) viewport: [f32; 4],
    /// The orthographic half height of the camera, when the snapshot holds it.
    pub(super) half_height: Option<f32>,
}

/// What a target names as attachments: the textures whose color buffers are drawn into, in order,
/// and the texture whose depth buffer is drawn with.
struct TargetSet {
    colors: Vec<TextureId>,
    depth: Option<TextureId>,
}

/// Whether a viewport of `rect` (x, y, width, height in pixels) covers a target of `size`.
pub(super) fn covers(rect: [f32; 4], size: [u32; 2]) -> bool {
    let [x, y, width, height] = rect.map(f64::from);
    let [target_width, target_height] = size.map(f64::from);
    x <= 0.0 && y <= 0.0 && x + width >= target_width && y + height >= target_height
}

/// The global values and keywords as the commands left them, and whether a record has yet to
/// carry the change.
struct Globals {
    layer: PropertyLayer,
    keywords: BTreeSet<String>,
    dirty: bool,
}

/// The renderers a culling record sorted, as indices into the snapshot's renderers.
#[derive(Default)]
pub(super) struct Culled {
    /// The renderers of the layer mask inside the view volume: what a drawing list draws from.
    pub(super) kept: Vec<usize>,
    /// The renderers of the layer mask wholly outside the view volume. Only a shadow list reads
    /// them, since a caster out of the camera's view can still shadow what it sees.
    pub(super) outside_view: Vec<usize>,
}

/// What the commands of a frame have set so far.
pub(super) struct Script {
    /// What the draws after a `SetRenderTarget` go to.
    pub(super) target: Option<Attachments>,
    /// The rectangle in pixels (x, y, width, height) the draws after a `SetViewport` cover on every
    /// attachment of the bound set, until a target or a camera setup binds targets again. `None` is
    /// the whole target.
    pub(super) viewport: Option<[f32; 4]>,
    /// The scissor and the depth bias of the draws that follow.
    pub(super) raster: RasterScript,
    pub(super) camera: Option<CameraSetup>,
    globals: Globals,
    pub(super) temporaries: Temporaries,
    /// The renderers each culling record's layer mask kept, as indices into the snapshot's
    /// renderers: those inside the camera's view volume, which a drawing list draws, and those
    /// outside it, from which a shadow list casts whether or not the camera sees them.
    pub(super) culls: BTreeMap<u64, Arc<Culled>>,
    /// The culling mask each culling record gave, which picks the main light of its draws.
    pub(super) cull_masks: BTreeMap<u64, u32>,
    /// The renderer lists made so far.
    pub(super) lists: BTreeMap<u64, RendererListDesc>,
    /// The renderers a list asked the frame to hold, and whether it could.
    pub(super) renderers: BTreeMap<usize, Result<(), RendererSkip>>,
    /// How many mesh draws have made a renderer of their own.
    pub(super) mesh_draws: u64,
    /// The screen of the host, if it has one.
    screen: Option<Screen>,
}

impl Script {
    /// The state at the start of a frame, with the globals the world holds.
    pub(super) fn new(
        globals: PropertyLayer,
        keywords: &[String],
        screen: Option<[u32; 2]>,
    ) -> Self {
        Self {
            target: None,
            viewport: None,
            raster: RasterScript::default(),
            camera: None,
            globals: Globals {
                layer: globals,
                keywords: keywords.iter().cloned().collect(),
                dirty: false,
            },
            temporaries: Temporaries::default(),
            culls: BTreeMap::new(),
            cull_masks: BTreeMap::new(),
            lists: BTreeMap::new(),
            renderers: BTreeMap::new(),
            mesh_draws: 0,
            screen: screen.map(Screen::new),
        }
    }
}

/// The name of a kind of work, for reports.
pub(super) fn kind(work: &OffscreenWork) -> &'static str {
    match work {
        OffscreenWork::CameraRender { .. } => "CameraRender",
        OffscreenWork::Blit { .. } => "Blit",
        OffscreenWork::CopyTexture { .. } => "CopyTexture",
        OffscreenWork::Dispatch { .. } => "Dispatch",
        OffscreenWork::SetRenderTarget { .. } => "SetRenderTarget",
        OffscreenWork::ClearRenderTarget { .. } => "ClearRenderTarget",
        OffscreenWork::SetGlobal(_) => "SetGlobal",
        OffscreenWork::SetGlobalKeyword { .. } => "SetGlobalKeyword",
        OffscreenWork::AllocateTemporary { .. } => "AllocateTemporary",
        OffscreenWork::ReleaseTemporary { .. } => "ReleaseTemporary",
        OffscreenWork::SetupCamera { .. } => "SetupCamera",
        OffscreenWork::Cull { .. } => "Cull",
        OffscreenWork::ReadPixels { .. } => "ReadPixels",
        OffscreenWork::CreateRendererList { .. } => "CreateRendererList",
        OffscreenWork::DrawRendererList { .. } => "DrawRendererList",
        OffscreenWork::SetViewProjection { .. } => "SetViewProjection",
        OffscreenWork::SetViewport { .. } => "SetViewport",
        OffscreenWork::EnableScissor { .. } => "EnableScissor",
        OffscreenWork::DisableScissor => "DisableScissor",
        OffscreenWork::SetDepthBias { .. } => "SetDepthBias",
        OffscreenWork::DrawMesh { .. } => "DrawMesh",
        OffscreenWork::DrawMeshBatch { .. } => "DrawMeshBatch",
        OffscreenWork::DrawProcedural { .. } => "DrawProcedural",
        OffscreenWork::GenerateMips { .. } => "GenerateMips",
    }
}

impl Mapper<'_> {
    /// Runs a command that sets the running state or makes a record from it.
    pub(super) fn command(&mut self, work: &OffscreenWork) -> Result<Vec<Record>, Skip> {
        match work {
            OffscreenWork::SetRenderTarget { .. }
            | OffscreenWork::SetGlobal(_)
            | OffscreenWork::SetGlobalKeyword { .. }
            | OffscreenWork::AllocateTemporary { .. }
            | OffscreenWork::ReleaseTemporary { .. }
            | OffscreenWork::SetupCamera { .. }
            | OffscreenWork::CreateRendererList { .. }
            | OffscreenWork::SetViewProjection { .. }
            | OffscreenWork::SetViewport { .. }
            | OffscreenWork::EnableScissor { .. }
            | OffscreenWork::DisableScissor
            | OffscreenWork::SetDepthBias { .. } => {
                self.set_state(work)?;
                Ok(Vec::new())
            }
            OffscreenWork::ClearRenderTarget {
                color,
                depth,
                stencil,
                value,
                depth_value,
                stencil_value,
            } => self.clear(
                (*color, *depth, *stencil),
                *value,
                (*depth_value, *stencil_value),
            ),
            OffscreenWork::DrawRendererList { list } => self.draw_list(*list),
            OffscreenWork::DrawMesh {
                mesh,
                matrix,
                material,
                submesh,
                pass,
                block,
                instances,
            } => self.draw_mesh(
                (*mesh, *submesh),
                matrix,
                (*material, *pass),
                (block.as_deref(), *instances),
            ),
            OffscreenWork::DrawMeshBatch {
                mesh,
                material,
                submesh,
                pass,
                batch,
            } => self.draw_mesh_batch((*mesh, *submesh), (*material, *pass), batch),
            OffscreenWork::DrawProcedural {
                matrix,
                material,
                pass,
                topology,
                vertex_count,
                instance_count,
            } => self.draw_procedural(
                matrix,
                (*material, *pass),
                *topology,
                (*vertex_count, *instance_count),
            ),
            OffscreenWork::GenerateMips { texture } => self.generate_mips(*texture),
            OffscreenWork::CameraRender { .. }
            | OffscreenWork::Cull { .. }
            | OffscreenWork::Blit { .. }
            | OffscreenWork::CopyTexture { .. }
            | OffscreenWork::Dispatch { .. }
            | OffscreenWork::ReadPixels { .. } => {
                unreachable!("{} is mapped elsewhere", kind(work))
            }
        }
    }

    /// Runs a command that changes the running state and makes no record.
    fn set_state(&mut self, work: &OffscreenWork) -> Result<(), Skip> {
        match work {
            OffscreenWork::SetRenderTarget {
                color,
                depth,
                mip,
                face,
                slice,
            } => self.set_render_target(color, depth.as_ref(), (*mip, *face, *slice))?,
            OffscreenWork::SetGlobal(value) => {
                if let WorldValue::Texture(name, _) = value {
                    self.report.global_reads.set.insert(name.clone());
                }
                let temporary = self.temporary_global(value)?;
                let globals = &mut self.script.globals;
                match temporary {
                    Some((name, input)) => {
                        globals.layer.none_textures.remove(&name);
                        globals.layer.textures.insert(name, input);
                    }
                    None => set_value(&mut globals.layer, value, &mut self.textures),
                }
                globals.dirty = true;
            }
            OffscreenWork::SetGlobalKeyword { name, enabled } => {
                let globals = &mut self.script.globals;
                let changed = if *enabled {
                    globals.keywords.insert(name.clone())
                } else {
                    globals.keywords.remove(name)
                };
                globals.dirty |= changed;
            }
            OffscreenWork::AllocateTemporary { name_id, desc } => {
                self.script.temporaries.allocate(*name_id, *desc);
            }
            OffscreenWork::ReleaseTemporary { name_id } => {
                if !self.script.temporaries.release(*name_id) {
                    return Err(Skip::UnknownTemporary);
                }
            }
            OffscreenWork::SetupCamera {
                camera,
                view,
                projection,
                target,
                buffers,
            } => self.setup_camera(*camera, (view, projection), (*target, *buffers))?,
            OffscreenWork::CreateRendererList { id, desc } => self.create_list(*id, desc),
            OffscreenWork::SetViewProjection { view, projection } => {
                self.set_view_projection(view, projection);
            }
            OffscreenWork::SetViewport { rect } => self.script.viewport = Some(*rect),
            raster if self.script.raster.apply(raster) => {}
            _ => unreachable!("{} makes a record", kind(work)),
        }
        Ok(())
    }

    /// The globals record that carries the changes since the last one, if there are any.
    pub(super) fn pending_globals(&mut self) -> Option<Record> {
        let globals = &mut self.script.globals;
        if !globals.dirty {
            return None;
        }
        globals.dirty = false;
        Some(Record::Globals(Arc::new(GlobalState {
            values: globals.layer.clone(),
            keywords: globals.keywords.iter().cloned().collect(),
        })))
    }

    /// The name and input of a global texture that names a temporary render texture, read with the
    /// sampler it was allocated with; `None` for any other value.
    fn temporary_global(&self, value: &WorldValue) -> Result<Option<(String, TextureInput)>, Skip> {
        let WorldValue::Texture(name, slot) = value else {
            return Ok(None);
        };
        let TextureRef::Temporary(name_id) = slot.texture else {
            return Ok(None);
        };
        let texture = self
            .script
            .temporaries
            .resolve(name_id)
            .ok_or(Skip::UnknownTemporary)?;
        let sampler = self
            .script
            .temporaries
            .described
            .get(&texture)
            .map(|desc| sampler_of_world(&desc.sampler));
        Ok(Some((name.clone(), TextureInput { texture, sampler })))
    }

    /// The texture a target names for `role`, or `None` for no target. The camera target is the
    /// target of the camera set up, and the screen when that camera has no texture or none is set
    /// up.
    pub(super) fn resolve_target(&self, target: &TargetRef) -> Result<Option<TextureId>, Skip> {
        match *target {
            TargetRef::None => Ok(None),
            TargetRef::Render(id) => {
                if self.snapshot.render_textures.contains_key(&id) {
                    Ok(Some(ids::render_texture(id)))
                } else {
                    Err(Skip::StaleTarget)
                }
            }
            TargetRef::Temporary(name_id) => self
                .script
                .temporaries
                .resolve(name_id)
                .map(Some)
                .ok_or(Skip::UnknownTemporary),
            TargetRef::CameraTarget => {
                match self.script.camera.as_ref().map(|camera| camera.target) {
                    Some(TargetRef::CameraTarget) | None => self.screen_texture().map(Some),
                    Some(own) => self.resolve_target(&own),
                }
            }
        }
    }

    /// What a target names as a set of attachments: the textures whose color buffers it draws
    /// into, in output order, and the texture whose depth buffer it draws with. A texture is its
    /// own color and depth. The camera's own target is the buffers the camera was set up with when
    /// it has them (their depth is the first color's when the script named none), else its target
    /// texture.
    fn resolve_set(&self, target: &TargetRef) -> Result<TargetSet, Skip> {
        let buffers = match target {
            TargetRef::CameraTarget => self.script.camera.as_ref().and_then(|c| c.buffers),
            _ => None,
        };
        let Some(buffers) = buffers else {
            let texture = self.resolve_target(target)?;
            return Ok(TargetSet {
                colors: texture.into_iter().collect(),
                depth: texture,
            });
        };
        let mut colors = Vec::new();
        for id in buffers.colors() {
            colors.extend(self.resolve_target(&TargetRef::Render(*id))?);
        }
        let depth = match buffers.depth() {
            Some(id) => self.resolve_target(&TargetRef::Render(id))?,
            None => colors.first().copied(),
        };
        Ok(TargetSet { colors, depth })
    }

    /// Refuses colors that one pass cannot draw into together: more than eight, one mip of a
    /// texture twice, or sizes that differ at the mip.
    fn check_colors(&self, colors: &[TextureId], mip: u32) -> Result<(), Skip> {
        if colors.len() > MAX_COLOR_TARGETS {
            return Err(Skip::TooManyColorTargets);
        }
        if colors
            .iter()
            .enumerate()
            .any(|(index, texture)| colors[..index].contains(texture))
        {
            return Err(Skip::RepeatedColorTarget);
        }
        let size = |texture: TextureId| {
            self.described(texture).map(|held| {
                [held.width, held.height].map(|side| side.checked_shr(mip).unwrap_or(0).max(1))
            })
        };
        if colors
            .iter()
            .any(|texture| size(*texture) != size(colors[0]))
        {
            return Err(Skip::ColorSizeMismatch);
        }
        Ok(())
    }

    /// The screen's texture, which serves as colour and as depth.
    fn screen_texture(&self) -> Result<TextureId, Skip> {
        match self.script.screen {
            None => Err(Skip::UnboundTarget),
            Some(_) => Ok(ids::screen_color()),
        }
    }

    /// The targets a clear goes to. A clear is refused while a recorded viewport leaves part of
    /// them uncovered, because the offscreen host clears a whole target. A draw takes
    /// [`draw_target`](Self::draw_target), which maps a partial viewport.
    pub(super) fn bound_target(&self) -> Result<Attachments, Skip> {
        let target = self.script.target.clone().ok_or(Skip::UnboundTarget)?;
        match (self.script.viewport, self.target_size()) {
            (None, _) => Ok(target),
            (Some(rect), Some(size)) if covers(rect, size) => Ok(target),
            (Some(_), _) => Err(Skip::PartialViewport),
        }
    }

    /// How many mips a texture of the frame has, or 0 for one that is not there.
    fn mips_of(&self, texture: TextureId) -> u32 {
        self.described(texture).map_or(0, |held| held.mip_count)
    }

    /// What a texture of the frame is: a temporary the commands allocated, the screen or a render
    /// texture of the snapshot.
    pub(super) fn described(&self, texture: TextureId) -> Option<&WorldRenderTexture> {
        self.held(texture).or_else(|| {
            ids::render_texture_of(texture).and_then(|id| self.snapshot.render_textures.get(&id))
        })
    }

    /// What a texture the frame holds is: a temporary the commands allocated or the screen.
    fn held(&self, texture: TextureId) -> Option<&WorldRenderTexture> {
        self.script
            .temporaries
            .described
            .get(&texture)
            .or_else(|| self.script.screen.as_ref()?.describe(texture))
    }

    /// The size in pixels of the bound target: of the color attachment's mip, or of the depth
    /// attachment's when there is no color. `None` when no target is bound.
    pub(super) fn target_size(&self) -> Option<[u32; 2]> {
        let target = self.script.target.as_ref()?;
        let attachment = target.colors.first().or(target.depth.as_ref())?;
        let held = self.described(attachment.texture)?;
        let level = |side: u32| side.checked_shr(attachment.mip).unwrap_or(0).max(1);
        Some([level(held.width), level(held.height)])
    }

    /// Binds the targets the draws after this go to: every color target, in order, which the
    /// fragment stage's outputs fill; a record that names none draws into the depth target alone,
    /// and one that names no depth target draws with the depth buffer of its first color. A
    /// color target that is the camera's own stands for all the buffers the camera was set up
    /// with.
    fn set_render_target(
        &mut self,
        color: &[TargetRef],
        depth: Option<&TargetRef>,
        (mip, face, slice): (u32, i32, i32),
    ) -> Result<(), Skip> {
        // A render texture here has one face and one slice.
        if face > 0 || slice > 0 {
            return Err(Skip::UnsupportedSlice);
        }
        let mut colors = Vec::new();
        let mut default_depth = None;
        let mut empty_slot = false;
        for (index, target) in color.iter().enumerate() {
            let set = self.resolve_set(target)?;
            if set.colors.is_empty() {
                empty_slot = true;
                continue;
            }
            // A named target after an empty one would take the wrong output.
            if empty_slot {
                return Err(Skip::EmptyColorSlot);
            }
            if index == 0 {
                default_depth = set.depth;
            }
            colors.extend(set.colors);
        }
        let depth_texture = match depth {
            Some(target) => self.resolve_set(target)?.depth,
            None => default_depth,
        };
        if colors.is_empty() && depth_texture.is_none() {
            return Err(Skip::UnboundTarget);
        }
        if colors
            .iter()
            .chain(&depth_texture)
            .any(|texture| mip >= self.mips_of(*texture))
        {
            return Err(Skip::NoSuchMip);
        }
        self.check_colors(&colors, mip)?;
        self.note_depth_size(&colors, depth_texture, mip);
        let at = |texture| Attachment { texture, mip };
        self.script.target = Some(Attachments {
            colors: colors.into_iter().map(at).collect(),
            depth: depth_texture.map(at),
        });
        self.script.viewport = None;
        self.script.raster.end_scissor();
        Ok(())
    }

    /// Counts a depth target of another texture than the first color's whose size differs from
    /// the color's: the pass is drawn without it, as a depth buffer that does not fit cannot go
    /// with the colors.
    fn note_depth_size(&mut self, colors: &[TextureId], depth: Option<TextureId>, mip: u32) {
        let (Some(&first), Some(depth)) = (colors.first(), depth) else {
            return;
        };
        let size = |texture: TextureId| {
            self.described(texture).map(|held| {
                [held.width, held.height].map(|side| side.checked_shr(mip).unwrap_or(0).max(1))
            })
        };
        if depth != first && size(depth) != size(first) {
            *self
                .report
                .census
                .entry("depth target left out: its size differs from the color's".to_owned())
                .or_default() += 1;
        }
    }

    fn clear(
        &self,
        (color, depth, stencil): (bool, bool, bool),
        value: [f32; 4],
        (depth_value, stencil_value): (f32, u32),
    ) -> Result<Vec<Record>, Skip> {
        let target = self.bound_target()?;
        if !(color || depth || stencil) {
            return Ok(Vec::new());
        }
        Ok(vec![Record::Clear {
            target,
            clear: Clear {
                color: color.then(|| value.map(f64::from)),
                depth: depth.then_some(depth_value),
                stencil: stencil.then_some(stencil_value),
            },
        }])
    }

    /// Sets up the camera. When its target is a texture, or it has buffers, the draws after this go
    /// to it; buffers that cannot be drawn into together refuse the setup and leave the draws
    /// with no target.
    fn setup_camera(
        &mut self,
        camera: ComponentHandle,
        (view, projection): (&[f32; 16], &[f32; 16]),
        (target, buffers): (TargetRef, Option<CameraBuffers>),
    ) -> Result<(), Skip> {
        // The world copies the cameras that camera renders name, so a camera set up alone may be
        // absent; it then has the whole target.
        let held = self.snapshot.cameras.get(&camera);
        let viewport = held.map_or([0.0, 0.0, 1.0, 1.0], |held| held.settings.viewport);
        self.script.camera = Some(CameraSetup {
            target,
            buffers,
            view: Mat4::from_cols_array(view),
            projection: Mat4::from_cols_array(projection),
            viewport,
            half_height: held.map(|held| held.settings.half_height),
        });
        self.script.viewport = None;
        self.script.raster.end_scissor();
        // A target that cannot be named leaves nothing bound, so the next clear or draw is refused
        // instead of reaching the target an earlier command bound.
        let set = match self.resolve_set(&target) {
            Ok(set) => set,
            Err(skip) => {
                self.script.target = None;
                // A camera of a screen the host lacks or of a texture that is gone has no target
                // to bind; only buffers that cannot be named refuse the setup.
                return if buffers.is_none() { Ok(()) } else { Err(skip) };
            }
        };
        if let Err(skip) = self.check_colors(&set.colors, 0) {
            self.script.target = None;
            return Err(skip);
        }
        if set.colors.is_empty() && set.depth.is_none() {
            self.script.target = None;
            return Ok(());
        }
        let whole = |texture| Attachment { texture, mip: 0 };
        self.note_depth_size(&set.colors, set.depth, 0);
        self.script.target = Some(Attachments {
            colors: set.colors.into_iter().map(whole).collect(),
            depth: set.depth.map(whole),
        });
        Ok(())
    }

    /// Replaces the view and projection of the camera set up, keeping its target and viewport.
    /// Before any setup the matrices go with the camera target and the whole viewport.
    fn set_view_projection(&mut self, view: &[f32; 16], projection: &[f32; 16]) {
        let (view, projection) = (
            Mat4::from_cols_array(view),
            Mat4::from_cols_array(projection),
        );
        match &mut self.script.camera {
            Some(setup) => {
                setup.view = view;
                setup.projection = projection;
            }
            none => {
                *none = Some(CameraSetup {
                    target: TargetRef::CameraTarget,
                    buffers: None,
                    view,
                    projection,
                    viewport: [0.0, 0.0, 1.0, 1.0],
                    half_height: None,
                });
            }
        }
    }

    /// The specs of the render textures the frame holds (its temporaries and the screen) that
    /// `used` names, which the frame makes.
    pub(super) fn held_specs(&self, used: &BTreeSet<TextureId>) -> Vec<RenderTextureSpec> {
        used.iter()
            .filter_map(|id| {
                let desc = self.held(*id)?;
                Some(RenderTextureSpec {
                    id: *id,
                    size: [desc.width, desc.height],
                    dimension: RenderDimension::D2,
                    graphics_format: desc.format,
                    depth_bits: u8::try_from(desc.depth_bits).unwrap_or(u8::MAX),
                    mips: desc.mip_count,
                    srgb: desc.srgb,
                    generation: 0,
                })
            })
            .collect()
    }
}
