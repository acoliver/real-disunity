//! The buffers a camera draws into when a script set them with `Camera.SetTargetBuffers`.
use super::commands::MAX_COLOR_TARGETS;
use super::{GraphicsError, RenderTextureId};
use crate::{ComponentDesc, ComponentHandle, World, WorldError};

/// The render textures a camera draws into instead of one target texture: the color buffers of one
/// to eight textures, in the order of the fragment stage's outputs, and the depth buffer of one
/// texture, which may be none of the colors'. A texture appears once among the colors.
///
/// A camera has either these or a target texture; setting one replaces the other, as Unity's
/// `SetTargetBuffers` and `targetTexture` do. The world keeps the ids as set and does not check
/// that they are still live (a texture may be destroyed later), except where a record names the
/// buffers. They sit beside the camera component rather than in its settings, which are passed by
/// value in many places and have no room for eight more ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraBuffers {
    colors: [RenderTextureId; MAX_COLOR_TARGETS],
    count: u8,
    depth: Option<RenderTextureId>,
}

impl CameraBuffers {
    /// Buffers of `colors` and `depth`.
    ///
    /// # Errors
    /// Fails when `colors` holds none, more than [`MAX_COLOR_TARGETS`] or one texture twice.
    pub fn new(
        colors: &[RenderTextureId],
        depth: Option<RenderTextureId>,
    ) -> Result<Self, GraphicsError> {
        let repeated = colors
            .iter()
            .enumerate()
            .any(|(index, id)| colors[..index].contains(id));
        if colors.is_empty() || colors.len() > MAX_COLOR_TARGETS || repeated {
            return Err(GraphicsError::InvalidCameraBuffers);
        }
        let mut held = [RenderTextureId::from_bits(0); MAX_COLOR_TARGETS];
        held[..colors.len()].copy_from_slice(colors);
        Ok(Self {
            colors: held,
            // At most eight, which `new` checked.
            count: u8::try_from(colors.len()).unwrap_or(u8::MAX),
            depth,
        })
    }

    /// The textures whose color buffers are drawn into, in output order.
    #[must_use]
    pub fn colors(&self) -> &[RenderTextureId] {
        &self.colors[..usize::from(self.count)]
    }

    /// The texture whose depth buffer is drawn with, if the script named one.
    #[must_use]
    pub fn depth(&self) -> Option<RenderTextureId> {
        self.depth
    }

    /// Whether the buffers are of `texture`, as a color or as the depth.
    #[must_use]
    pub fn names(&self, texture: RenderTextureId) -> bool {
        self.colors().contains(&texture) || self.depth == Some(texture)
    }
}

impl World {
    /// Gives a camera the buffers it draws into, or removes them with `None`. The camera's target
    /// texture is cleared when buffers are set, as `SetTargetBuffers` replaces `targetTexture`; a
    /// later write of a target texture to the component is for the caller to pair with
    /// [`World::set_camera_buffers`] of `None`.
    ///
    /// # Errors
    /// Fails when `camera` is not a live camera component or a texture of the buffers is not a live
    /// render texture, and changes nothing then.
    pub fn set_camera_buffers(
        &mut self,
        camera: ComponentHandle,
        buffers: Option<CameraBuffers>,
    ) -> Result<(), WorldError> {
        let ComponentDesc::Camera(mut settings) = self.component(camera)?.desc else {
            return Err(GraphicsError::NotCamera.into());
        };
        let Some(buffers) = buffers else {
            self.graphics.camera_buffers.remove(&camera);
            return Ok(());
        };
        for texture in buffers.colors().iter().chain(&buffers.depth()) {
            self.render_texture(*texture)?;
        }
        settings.target_texture = None;
        self.set_component(camera, ComponentDesc::Camera(settings))?;
        // Entries of cameras that were retired go with the next set, so the table stays as small
        // as the cameras scripts have used.
        let retired: Vec<_> = self
            .graphics
            .camera_buffers
            .keys()
            .filter(|held| self.component(**held).is_err())
            .copied()
            .collect();
        for held in retired {
            self.graphics.camera_buffers.remove(&held);
        }
        self.graphics.camera_buffers.insert(camera, buffers);
        Ok(())
    }

    /// The buffers a live camera draws into, if its script set them.
    #[must_use]
    pub fn camera_buffers(&self, camera: ComponentHandle) -> Option<CameraBuffers> {
        let held = self.graphics.camera_buffers.get(&camera)?;
        self.component(camera).ok().map(|_| *held)
    }
}
