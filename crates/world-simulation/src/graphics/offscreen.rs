//! Offscreen work: the draws into render textures that scripts ask for during a frame.
use super::readback::Rect;
use super::renderer_lists::RendererListDesc;
use super::{
    ComputeBindings, ComputeShaderId, GraphicsError, MaterialId, PropertyValue, RenderTextureId,
    TargetRef, TextureRef, WorldRenderTexture,
};
use crate::{ComponentDesc, ComponentHandle, World, WorldError};

/// Most records one frame holds.
pub const MAX_OFFSCREEN_WORK: usize = 1 << 16;

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
    /// the draw copies. A pass of -1 draws every pass.
    Blit {
        source: TextureRef,
        target: RenderTextureId,
        material: Option<MaterialId>,
        pass: i32,
    },
    /// A copy of one element and mip level of a texture into one of a render texture.
    CopyTexture {
        source: TextureRef,
        source_element: u32,
        source_mip: u32,
        target: RenderTextureId,
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
    SetupCamera {
        camera: ComponentHandle,
        view: [f32; 16],
        projection: [f32; 16],
        target: TargetRef,
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

    /// Checks a record and appends it to the frame's list.
    pub(super) fn push_offscreen(&mut self, work: OffscreenWork) -> Result<(), WorldError> {
        self.check_offscreen(&work)?;
        if self.graphics.offscreen.len() >= MAX_OFFSCREEN_WORK {
            return Err(WorldError::CapacityExceeded(crate::LimitKind::Graphics));
        }
        self.note_handle(&work);
        self.graphics.offscreen.push(work);
        Ok(())
    }

    /// The frame's offscreen work so far, in call order.
    pub fn offscreen_work(&self) -> &[OffscreenWork] {
        &self.graphics.offscreen
    }

    /// Drains the frame's offscreen work, in call order. A frame that starts with work nobody
    /// took drops it, so a host that never draws offscreen does not accumulate records.
    pub fn take_offscreen(&mut self) -> Vec<OffscreenWork> {
        self.graphics.handles.clear();
        self.graphics.lists.clear();
        self.graphics.retired_textures.clear();
        std::mem::take(&mut self.graphics.offscreen)
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
                self.check_texture(source)?;
                self.render_texture(*target)?;
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
                self.check_texture(source)?;
                self.check_mip(*target, *target_mip)?;
                match source {
                    TextureRef::Render(id) => self.check_mip(*id, *source_mip),
                    TextureRef::Asset(_) | TextureRef::None | TextureRef::Global(_) => Ok(()),
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
            OffscreenWork::CreateRendererList { .. } | OffscreenWork::DrawRendererList { .. } => {
                self.check_renderer_list(work)
            }
            _ => self
                .check_command(work)
                .and_then(|()| self.check_context(work)),
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
