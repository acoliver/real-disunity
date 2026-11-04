//! Offscreen work: the draws into render textures that scripts ask for during a frame.
use super::{GraphicsError, MaterialId, RenderTextureId, TextureRef};
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
}

impl World {
    /// Appends offscreen work to the frame's list. Everything the record names must be alive:
    /// the camera a live camera component, the render textures and the material live resources,
    /// and a mip level must exist in the render texture it addresses.
    pub fn record_offscreen(&mut self, work: OffscreenWork) -> Result<(), WorldError> {
        self.check_offscreen(&work)?;
        if self.graphics.offscreen.len() >= MAX_OFFSCREEN_WORK {
            return Err(WorldError::CapacityExceeded(crate::LimitKind::Graphics));
        }
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
        std::mem::take(&mut self.graphics.offscreen)
    }

    fn check_offscreen(&self, work: &OffscreenWork) -> Result<(), WorldError> {
        match *work {
            OffscreenWork::CameraRender { camera, target } => {
                if !matches!(self.component(camera)?.desc, ComponentDesc::Camera(_)) {
                    return Err(GraphicsError::NotCamera.into());
                }
                self.render_texture(target).map(drop)
            }
            OffscreenWork::Blit {
                source,
                target,
                material,
                pass,
            } => {
                self.check_texture(source)?;
                self.render_texture(target)?;
                if let Some(material) = material {
                    self.material(material)?;
                }
                if pass < -1 {
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
                self.check_mip(target, target_mip)?;
                match source {
                    TextureRef::Render(id) => self.check_mip(id, source_mip),
                    TextureRef::Asset(_) | TextureRef::None => Ok(()),
                }
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
