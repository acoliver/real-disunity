//! Offscreen work: the draws into render textures that scripts ask for during a frame.
use std::collections::BTreeMap;

use super::draw_batch::DrawBatch;
use super::readback::Rect;
use super::renderer_lists::RendererListDesc;
use super::{
    CameraBuffers, ComputeBindings, ComputeShaderId, GlobalShaderState, GraphicsError, MaterialId,
    PropertyValue, RenderTextureId, ShaderProperties, TargetRef, TextureRef, WorldMaterial,
    WorldRenderTexture,
};
use crate::{ComponentDesc, ComponentHandle, World, WorldError};

/// Most records the world holds between two takes of its offscreen work. A take, not a frame, empties
/// them, so one take can hold all the rendering a script does while a level or save loads: loading
/// Old World's `RomeTurn42` save records 106,943 between two takes, with 684 camera renders. The bound
/// keeps a script that records without end from growing the list without limit.
pub const MAX_OFFSCREEN_WORK: usize = 1 << 20;

/// One piece of offscreen work, in the order the script called it.
#[derive(Clone, Debug, PartialEq)]
pub enum OffscreenWork {
    /// A camera draws its view into a render texture. The camera's culling mask, clear flags and
    /// pose are read from the world when the host executes the record.
    CameraRender {
        camera: ComponentHandle,
        target: RenderTextureId,
    },
    /// A full-surface draw of `source` into `target` with a material pass; without a material
    /// the draw copies. A pass of -1 draws every pass. The target is a render texture or a
    /// temporary one; a source that is the target reads it as it was before the blit.
    Blit {
        source: TextureRef,
        target: TargetRef,
        material: Option<MaterialId>,
        pass: i32,
    },
    /// A copy of one element and mip level of a texture into one of a render texture or a
    /// temporary one.
    CopyTexture {
        source: TextureRef,
        source_element: u32,
        source_mip: u32,
        target: TargetRef,
        target_element: u32,
        target_mip: u32,
    },
    /// A dispatch of one kernel of a compute shader over `groups` thread groups. `bindings` is
    /// the snapshot of the values the kernel ran with when the script called it; a script's
    /// later edits leave it alone. A texture it holds is an asset, a render texture or none,
    /// never a global name.
    Dispatch {
        shader: ComputeShaderId,
        kernel: u32,
        groups: [u32; 3],
        bindings: ComputeBindings,
    },
    /// Draws after this record go to these targets. `mip`, `face` (-1 for every face) and
    /// `slice` (-1 for every slice) address the level of each target. A record names one to
    /// eight color targets, or none when it names a depth target.
    SetRenderTarget {
        color: Vec<TargetRef>,
        depth: Option<TargetRef>,
        mip: u32,
        face: i32,
        slice: i32,
    },
    /// Clears the bound targets. The color is cleared to `value`, the depth to `depth_value` and
    /// the stencil to `stencil_value`, for the parts that are set.
    ClearRenderTarget {
        color: bool,
        depth: bool,
        stencil: bool,
        value: [f32; 4],
        depth_value: f32,
        stencil_value: u32,
    },
    /// Sets a global shader value when the host reaches this record.
    SetGlobal(PropertyValue),
    /// Turns a global shader keyword on or off when the host reaches this record.
    SetGlobalKeyword { name: String, enabled: bool },
    /// Creates a temporary render texture under a property name id. `desc.generation` is
    /// ignored. The host frees it at the matching release, or when its frame ends.
    AllocateTemporary {
        name_id: i32,
        desc: WorldRenderTexture,
    },
    /// Releases the temporary render texture of a name id.
    ReleaseTemporary { name_id: i32 },
    /// Draws after this record use the camera's view and projection (column-major, as the world
    /// holds them) and go to `target`, as the script's setup of the camera properties asked. The
    /// camera supplies the viewport and clear settings when the host executes the record.
    ///
    /// `buffers` are the camera's target buffers as the script set them when it made the call,
    /// which the target is [`TargetRef::CameraTarget`] for: the camera's own target then is these
    /// color textures and this depth texture, and the record does not depend on what the camera's
    /// buffers are when the host executes it.
    SetupCamera {
        camera: ComponentHandle,
        view: [f32; 16],
        projection: [f32; 16],
        target: TargetRef,
        buffers: Option<CameraBuffers>,
    },
    /// A culling of the world's renderers for a camera. `id` names the result for later records of
    /// the frame (see [`World::record_offscreen`]); the host culls with the camera's `culling_mask`
    /// (layers), the `planes` of the view volume (each a unit normal and a distance, up to
    /// [`MAX_CULL_PLANES`]) and the culling `options` flags as the script gave them.
    Cull {
        id: u64,
        camera: ComponentHandle,
        culling_mask: u32,
        planes: Vec<[f32; 4]>,
        options: u32,
    },
    /// A read of `width` by `height` texels of mip 0 of `texture`, from the bottom left texel at
    /// (`x`, `y`), after the work recorded before this record has drawn. The host answers with
    /// `width * height * 4` floats, bottom row first, as the texture stores them (see
    /// [`World::request_texture_pixels`]). `request` is the id that call issued.
    ReadPixels {
        request: u64,
        texture: RenderTextureId,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// A renderer list: what a culling result's renderers (or one light's shadow casters) draw
    /// under the list's filters. `id` names it for later records of the frame. It is made only
    /// by [`World::create_renderer_list`].
    CreateRendererList { id: u64, desc: RendererListDesc },
    /// Draws a renderer list created earlier in the frame into the bound targets.
    DrawRendererList { list: u64 },
    /// The view and projection the draws that follow use, until the next camera setup or view
    /// change: two column-major matrices, as a camera setup carries them. It names no camera and
    /// leaves the bound targets as they are.
    SetViewProjection {
        view: [f32; 16],
        projection: [f32; 16],
    },
    /// The rectangle, in pixels of the bound targets, that the draws after this record cover:
    /// `rect` is x, y, width and height from the bottom left texel. A target setting or a camera
    /// setup resets it to the whole target. The world checks that every component is finite and
    /// the width and height are not negative.
    SetViewport { rect: [f32; 4] },
    /// Clips the draws that follow to `rect`, in pixels of the bound targets: x, y, width and
    /// height from the bottom left texel, the convention of [`SetViewport`](Self::SetViewport).
    /// The clip holds until a [`DisableScissor`](Self::DisableScissor), a target setting or a
    /// camera setup. The world checks that every component is finite and the width and height are
    /// not negative.
    EnableScissor { rect: [f32; 4] },
    /// Ends the clip an [`EnableScissor`](Self::EnableScissor) set: the draws that follow cover
    /// the whole target again.
    DisableScissor,
    /// The depth bias of the draws that follow, until the next record of this kind: `bias` is the
    /// constant offset and `slope_bias` the factor of the triangle's depth slope, as the script
    /// gave them. A bias of 0 and a slope bias of 0 end it. The world checks that both are finite.
    SetDepthBias { bias: f32, slope_bias: f32 },
    /// Draws the mesh the renderer identity `mesh` names, with `matrix` (column-major, from the
    /// mesh's coordinates to the world) and the material's pass, into the bound targets with the
    /// current view and projection. `submesh` picks one sub-mesh; a `pass` of -1 draws every
    /// pass. The world keeps no mesh contents (the host reads them by identity), so it checks
    /// that the identity is nonzero and leaves an identity nothing stored to the host. `block` is
    /// the draw's own property block, read between its per-draw values and the material, as a
    /// renderer's block is; the world checks that the render textures it names are alive.
    /// `instances` is how many instances the draw makes (1 for a plain draw). Zero is a draw of
    /// nothing that keeps its place among the records, as a procedural draw of a zero count does.
    DrawMesh {
        mesh: u64,
        matrix: [f32; 16],
        material: MaterialId,
        submesh: u32,
        pass: i32,
        block: Option<Box<ShaderProperties>>,
        instances: u32,
    },
    /// Draws instances of one sub-mesh of the mesh the identity `mesh` names, with the material's
    /// pass (-1 for every pass), into the bound targets with the current view and projection. The
    /// shader reads each instance's data from `batch`: the world checks that the buffer is live,
    /// that the window lies inside it and that the lists are within their limits, and leaves the
    /// meaning of the bytes to the shader. A record with no visible instance keeps its place among
    /// the records and draws nothing.
    DrawMeshBatch {
        mesh: u64,
        material: MaterialId,
        submesh: u32,
        pass: i32,
        batch: DrawBatch,
    },
    /// Draws `vertex_count` generated vertices `instance_count` times with `matrix`
    /// (column-major) and the material's pass into the bound targets, with the current view and
    /// projection. The shader makes each vertex from its vertex and instance index, so no mesh is
    /// read. A `pass` of -1 draws every pass. A zero count is a draw of nothing that keeps its
    /// place among the records.
    DrawProcedural {
        matrix: [f32; 16],
        material: MaterialId,
        pass: i32,
        topology: DrawTopology,
        vertex_count: u32,
        instance_count: u32,
    },
    /// Fills mip levels 1 and up of `texture` from level 0, as a script's explicit request for the
    /// texture's mips. The texture must have more than one mip level.
    GenerateMips { texture: RenderTextureId },
}

/// How a procedural draw joins its generated vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawTopology {
    /// Every three vertices make a triangle.
    TriangleList,
    /// Every two vertices make a line.
    LineList,
}

impl OffscreenWork {
    /// Whether the record reads or draws into the render texture, directly or through a texture
    /// property it sets.
    pub fn names_render_texture(&self, id: RenderTextureId) -> bool {
        let texture = |texture: &TextureRef| *texture == TextureRef::Render(id);
        let target = |target: &TargetRef| *target == TargetRef::Render(id);
        match self {
            Self::CameraRender { target, .. } => *target == id,
            Self::Blit { source, target, .. } | Self::CopyTexture { source, target, .. } => {
                *target == TargetRef::Render(id) || texture(source)
            }
            Self::Dispatch { bindings, .. } => bindings
                .properties
                .textures
                .iter()
                .any(|(_, slot)| texture(&slot.texture)),
            Self::DrawMesh {
                block: Some(block), ..
            } => block
                .textures
                .iter()
                .any(|(_, slot)| texture(&slot.texture)),
            Self::SetRenderTarget { color, depth, .. } => {
                color.iter().any(target) || depth.as_ref().is_some_and(target)
            }
            Self::SetGlobal(PropertyValue::Texture(_, slot)) => texture(&slot.texture),
            Self::SetupCamera {
                target: named,
                buffers,
                ..
            } => target(named) || buffers.is_some_and(|buffers| buffers.names(id)),
            Self::ReadPixels { texture, .. } | Self::GenerateMips { texture } => *texture == id,
            _ => false,
        }
    }
}

impl World {
    /// Appends offscreen work to the frame's list. Everything the record names must be alive:
    /// the camera a live camera component, the render textures and the material live resources,
    /// and a mip level must exist in the render texture it addresses. A dispatch names a live
    /// compute shader, one of its kernels and 1 to 65535 thread groups along each axis. A record
    /// that issues a handle (a culling result) takes a nonzero id no record of the frame took
    /// before; the frame forgets its handles when its work is taken. A read of pixels is
    /// recorded by [`World::request_texture_pixels`], which issues its request id, and a renderer list
    /// by [`World::create_renderer_list`], which issues its list id.
    pub fn record_offscreen(&mut self, work: OffscreenWork) -> Result<(), WorldError> {
        if matches!(
            work,
            OffscreenWork::ReadPixels { .. } | OffscreenWork::CreateRendererList { .. }
        ) {
            return Err(GraphicsError::InvalidOffscreen.into());
        }
        self.push_offscreen(work)
    }

    /// Checks a record and appends it to the frame's list, after a record for each global value
    /// or keyword written directly since the previous record. This is the one place every record
    /// is appended, so a host that replays the list in order sees a global at the value the
    /// script had set when it recorded the work, whatever the state holds by the flush. A record
    /// the frame has no room for, with its globals, is refused whole and leaves them noted.
    pub(super) fn push_offscreen(&mut self, mut work: OffscreenWork) -> Result<(), WorldError> {
        self.check_offscreen(&work)?;
        let ordered = self.ordered_global_records();
        if self.graphics.offscreen.len() + ordered.len() >= MAX_OFFSCREEN_WORK {
            return Err(WorldError::CapacityExceeded(crate::LimitKind::Graphics));
        }
        // The record follows the ordered globals, so the set is keyed by the record's own index.
        let record = self.graphics.offscreen.len() + ordered.len();
        self.capture_render_set(&work, record);
        self.note_handle(&work);
        self.graphics.direct_writes.clear();
        self.graphics.offscreen.extend(ordered);
        self.note_material_draw(&mut work);
        self.graphics.offscreen.push(work);
        Ok(())
    }

    /// The frame's offscreen work so far, in call order.
    pub fn offscreen_work(&self) -> &[OffscreenWork] {
        &self.graphics.offscreen
    }

    /// Drains the frame's offscreen work, in call order, with the materials its records name. A
    /// frame that starts with work nobody took drops it, so a host that never draws offscreen does
    /// not accumulate records.
    ///
    /// A blit, mesh draw or procedural draw reads the material as it stood when the script
    /// recorded it, whatever the script changes on the material afterwards. It names the material
    /// by its own id while the material still holds that state. When the script edits the material,
    /// the draws before the edit name a copy of the state they saw instead, under an id of
    /// generation 0 that no material of the world has. The map holds those copies, each under the
    /// id the records use; it is empty for a frame whose draws saw one state of each material.
    ///
    /// The third value is the global state the records replay from: the state as it was before
    /// the first change since the previous take, or `None` when nothing changed and the world's
    /// state is the start. Globals written after the last record are ordered after it, one
    /// record each, so a draw reads the value its global had when the script recorded it, whatever
    /// the state holds by the time of the flush. Work with no record has nothing to order them
    /// with: they are dropped, and the next take starts from the state that holds them. The
    /// trailing records can pass [`MAX_OFFSCREEN_WORK`] by the number of names written since
    /// the last record, which the world's global state bounds.
    pub fn take_offscreen(
        &mut self,
    ) -> (
        Vec<OffscreenWork>,
        BTreeMap<MaterialId, WorldMaterial>,
        Option<GlobalShaderState>,
    ) {
        let mut work = std::mem::take(&mut self.graphics.offscreen);
        if !work.is_empty() {
            work.extend(self.ordered_global_records());
        }
        self.graphics.direct_writes.clear();
        self.graphics.handles.clear();
        self.graphics.lists.clear();
        self.graphics.retired_textures.clear();
        self.graphics.render_sets.clear();
        let start = self.graphics.take_global_start();
        let copies = self.settle_material_versions(&mut work);
        (work, copies, start)
    }

    fn check_offscreen(&self, work: &OffscreenWork) -> Result<(), WorldError> {
        match work {
            OffscreenWork::CameraRender { camera, target } => {
                if !matches!(self.component(*camera)?.desc, ComponentDesc::Camera(_)) {
                    return Err(GraphicsError::NotCamera.into());
                }
                self.render_texture(*target).map(drop)
            }
            OffscreenWork::Blit {
                source,
                target,
                material,
                pass,
            } => {
                self.check_work_texture(source)?;
                self.check_work_target(target, 0)?;
                if let Some(material) = material {
                    self.material(*material)?;
                }
                if *pass < -1 {
                    return Err(GraphicsError::InvalidOffscreen.into());
                }
                Ok(())
            }
            OffscreenWork::CopyTexture {
                source,
                source_mip,
                target,
                target_mip,
                ..
            } => {
                self.check_work_texture(source)?;
                self.check_work_target(target, *target_mip)?;
                match source {
                    TextureRef::Render(id) => self.check_mip(*id, *source_mip),
                    TextureRef::Asset(_)
                    | TextureRef::None
                    | TextureRef::Unsent
                    | TextureRef::Global(_)
                    | TextureRef::Temporary(_) => Ok(()),
                }
            }
            OffscreenWork::Dispatch {
                shader,
                kernel,
                groups,
                bindings,
            } => self.check_dispatch(*shader, *kernel, *groups, bindings),
            OffscreenWork::ReadPixels {
                texture,
                x,
                y,
                width,
                height,
                ..
            } => self
                .check_read_rect(
                    *texture,
                    Rect {
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                    },
                )
                .map(drop),
            OffscreenWork::GenerateMips { texture } => self.check_mip(*texture, 1),
            OffscreenWork::CreateRendererList { .. } | OffscreenWork::DrawRendererList { .. } => {
                self.check_renderer_list(work)
            }
            _ => self
                .check_command(work)
                .and_then(|()| self.check_context(work)),
        }
    }

    /// Checks the target of a blit or a copy: a live render texture that has the mip level, or a
    /// temporary under any name id, which the host resolves when it reaches the record.
    fn check_work_target(&self, target: &TargetRef, mip: u32) -> Result<(), WorldError> {
        match target {
            TargetRef::Render(id) => self.check_mip(*id, mip),
            TargetRef::Temporary(_) => Ok(()),
            TargetRef::CameraTarget | TargetRef::None => {
                Err(GraphicsError::InvalidOffscreen.into())
            }
        }
    }

    fn check_mip(&self, texture: RenderTextureId, mip: u32) -> Result<(), WorldError> {
        if mip < self.render_texture(texture)?.mip_count {
            Ok(())
        } else {
            Err(GraphicsError::InvalidOffscreen.into())
        }
    }
}
