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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use glam::Mat4;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, GlobalState, Record, RenderDimension, RenderTextureSpec,
};
use worldrunner_world_simulation::ComponentHandle;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RendererListDesc, TargetRef, WorldRenderTexture,
};

use super::frame::{Mapper, RendererSkip, Skip};
use super::ids;
use super::properties::set_value;

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

    fn resolve(&self, name_id: i32) -> Option<TextureId> {
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

/// The camera a script set up: where its draws go when a target names the camera's own, the view
/// and the projection (column-major OpenGL matrices, as the world holds them) the draws use, and
/// the camera's rectangle of its target.
pub(super) struct CameraSetup {
    pub(super) target: TargetRef,
    pub(super) view: Mat4,
    pub(super) projection: Mat4,
    pub(super) viewport: [f32; 4],
}

/// The global values and keywords as the commands left them, and whether a record has yet to
/// carry the change.
struct Globals {
    layer: PropertyLayer,
    keywords: BTreeSet<String>,
    dirty: bool,
}

/// What the commands of a frame have set so far.
pub(super) struct Script {
    /// What the draws after a `SetRenderTarget` go to.
    pub(super) target: Option<Attachments>,
    pub(super) camera: Option<CameraSetup>,
    globals: Globals,
    pub(super) temporaries: Temporaries,
    /// The renderers each culling record kept, as indices into the snapshot's renderers.
    pub(super) culls: BTreeMap<u64, Vec<usize>>,
    /// The renderer lists made so far.
    pub(super) lists: BTreeMap<u64, RendererListDesc>,
    /// The renderers a list asked the frame to hold, and whether it could.
    pub(super) renderers: BTreeMap<usize, Result<(), RendererSkip>>,
}

impl Script {
    /// The state at the start of a frame, with the globals the world holds.
    pub(super) fn new(globals: PropertyLayer, keywords: &[String]) -> Self {
        Self {
            target: None,
            camera: None,
            globals: Globals {
                layer: globals,
                keywords: keywords.iter().cloned().collect(),
                dirty: false,
            },
            temporaries: Temporaries::default(),
            culls: BTreeMap::new(),
            lists: BTreeMap::new(),
            renderers: BTreeMap::new(),
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
        OffscreenWork::DrawMesh { .. } => "DrawMesh",
    }
}

impl Mapper<'_> {
    /// Runs a command that sets the running state or makes a record from it.
    pub(super) fn command(&mut self, work: &OffscreenWork) -> Result<Vec<Record>, Skip> {
        match work {
            OffscreenWork::SetRenderTarget {
                color,
                depth,
                mip,
                face,
                slice,
            } => {
                self.set_render_target(color, depth.as_ref(), (*mip, *face, *slice))?;
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
            OffscreenWork::SetGlobal(value) => {
                let globals = &mut self.script.globals;
                set_value(&mut globals.layer, value, &mut self.textures);
                globals.dirty = true;
                Ok(Vec::new())
            }
            OffscreenWork::SetGlobalKeyword { name, enabled } => {
                let globals = &mut self.script.globals;
                let changed = if *enabled {
                    globals.keywords.insert(name.clone())
                } else {
                    globals.keywords.remove(name)
                };
                globals.dirty |= changed;
                Ok(Vec::new())
            }
            OffscreenWork::AllocateTemporary { name_id, desc } => {
                self.script.temporaries.allocate(*name_id, *desc);
                Ok(Vec::new())
            }
            OffscreenWork::ReleaseTemporary { name_id } => {
                if self.script.temporaries.release(*name_id) {
                    Ok(Vec::new())
                } else {
                    Err(Skip::UnknownTemporary)
                }
            }
            OffscreenWork::SetupCamera {
                camera,
                view,
                projection,
                target,
            } => {
                self.setup_camera(*camera, (view, projection), *target);
                Ok(Vec::new())
            }
            OffscreenWork::Cull {
                id,
                culling_mask,
                planes,
                ..
            } => {
                self.cull(*id, *culling_mask, planes);
                Ok(Vec::new())
            }
            OffscreenWork::CreateRendererList { id, desc } => {
                self.create_list(*id, desc);
                Ok(Vec::new())
            }
            OffscreenWork::DrawRendererList { list } => self.draw_list(*list),
            OffscreenWork::SetViewProjection { view, projection } => {
                self.set_view_projection(view, projection);
                Ok(Vec::new())
            }
            OffscreenWork::DrawMesh { .. } => Err(Skip::MeshDrawNotDrawn),
            OffscreenWork::CameraRender { .. }
            | OffscreenWork::Blit { .. }
            | OffscreenWork::CopyTexture { .. }
            | OffscreenWork::Dispatch { .. }
            | OffscreenWork::ReadPixels { .. } => {
                unreachable!("{} is mapped elsewhere", kind(work))
            }
        }
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

    /// The texture a target names, or `None` for no target.
    fn resolve_target(&self, target: &TargetRef) -> Result<Option<TextureId>, Skip> {
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
                    Some(TargetRef::CameraTarget) | None => Err(Skip::UnboundTarget),
                    Some(own) => self.resolve_target(&own),
                }
            }
        }
    }

    /// How many mips a texture of the frame has, or 0 for one that is not there.
    fn mips_of(&self, texture: TextureId) -> u32 {
        if let Some(desc) = self.script.temporaries.described.get(&texture) {
            return desc.mip_count;
        }
        ids::render_texture_of(texture)
            .and_then(|id| self.snapshot.render_textures.get(&id))
            .map_or(0, |held| held.mip_count)
    }

    /// Binds the targets the draws after this go to. The first color target is the one drawn
    /// into; a record that names none draws into the depth target alone, and one that names no
    /// depth target draws with the depth buffer of its color.
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
        let first = match color.first() {
            Some(target) => self.resolve_target(target)?,
            None => None,
        };
        let depth_texture = match depth {
            Some(target) => self.resolve_target(target)?,
            None => first,
        };
        if first.is_none() && depth_texture.is_none() {
            return Err(Skip::UnboundTarget);
        }
        if first
            .iter()
            .chain(&depth_texture)
            .any(|texture| mip >= self.mips_of(*texture))
        {
            return Err(Skip::NoSuchMip);
        }
        if color.len() > 1 {
            *self
                .report
                .census
                .entry("color targets of a SetRenderTarget left out".to_owned())
                .or_default() += color.len() - 1;
        }
        let at = |texture| Attachment { texture, mip };
        self.script.target = Some(Attachments {
            color: first.map(at),
            depth: depth_texture.map(at),
        });
        Ok(())
    }

    fn clear(
        &self,
        (color, depth, stencil): (bool, bool, bool),
        value: [f32; 4],
        (depth_value, stencil_value): (f32, u32),
    ) -> Result<Vec<Record>, Skip> {
        let target = self.script.target.ok_or(Skip::UnboundTarget)?;
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

    /// Sets up the camera. When its target is a texture, the draws after this go to it.
    fn setup_camera(
        &mut self,
        camera: ComponentHandle,
        (view, projection): (&[f32; 16], &[f32; 16]),
        target: TargetRef,
    ) {
        // The world copies the cameras that camera renders name, so a camera set up alone may be
        // absent; it then has the whole target.
        let viewport = self
            .snapshot
            .cameras
            .get(&camera)
            .map_or([0.0, 0.0, 1.0, 1.0], |held| held.settings.viewport);
        self.script.camera = Some(CameraSetup {
            target,
            view: Mat4::from_cols_array(view),
            projection: Mat4::from_cols_array(projection),
            viewport,
        });
        if let Ok(Some(texture)) = self.resolve_target(&target) {
            let whole = Some(Attachment { texture, mip: 0 });
            self.script.target = Some(Attachments {
                color: whole,
                depth: whole,
            });
        }
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
                    view,
                    projection,
                    viewport: [0.0, 0.0, 1.0, 1.0],
                });
            }
        }
    }

    /// The specs of the temporary render textures that `records` use, which the frame makes.
    pub(super) fn temporary_specs(&self, used: &BTreeSet<TextureId>) -> Vec<RenderTextureSpec> {
        used.iter()
            .filter_map(|id| {
                let desc = self.script.temporaries.described.get(id)?;
                Some(RenderTextureSpec {
                    id: *id,
                    size: [desc.width, desc.height],
                    layers: 1,
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
