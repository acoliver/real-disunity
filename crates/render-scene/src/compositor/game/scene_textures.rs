//! The resident scene textures a game's passes sample: the textures the stock renderer uploaded
//! for the scene the frame draws, lent by scene texture handle so a game never uploads a copy.
use super::lent::{FrameInputError, Lent};
use std::rc::Rc;
use worldrunner_render_plan::TextureHandle;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::Import;

/// Why a lent scene texture cannot be bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameSceneTextureError {
    #[error("scene texture {handle} is not in the resident table of {resident}")]
    Unknown { handle: u32, resident: usize },
}

/// The resident textures of the scene the frame in flight draws, indexed by scene texture
/// handle: every texture of the scene's table, uploaded once from its prepared payload (with any
/// texture tier the host chose), including those appended since. One snapshot is shared by every
/// frame until the table changes.
#[derive(Clone, Debug)]
pub struct FrameSceneTextures(Rc<[wgpu::Texture]>);

impl FrameSceneTextures {
    pub(crate) fn new(textures: Rc<[wgpu::Texture]>) -> Self {
        Self(textures)
    }

    /// Textures in the resident table.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The texture of `handle`, through its default view, for the binding sourced from
    /// `semantic`.
    ///
    /// # Errors
    /// [`FrameSceneTextureError::Unknown`] for a handle beyond the resident table.
    pub fn import(
        &self,
        semantic: EngineSemantic,
        handle: TextureHandle,
    ) -> Result<Import, FrameSceneTextureError> {
        let texture = usize::try_from(handle.raw())
            .ok()
            .and_then(|index| self.0.get(index))
            .ok_or(FrameSceneTextureError::Unknown {
                handle: handle.raw(),
                resident: self.0.len(),
            })?;
        Ok(Import::texture(
            semantic,
            texture.clone(),
            texture.create_view(&wgpu::TextureViewDescriptor::default()),
        ))
    }

    /// Whether `other` is the same snapshot: the same table, so anything built from one binds
    /// the textures the other would. An append or a texture replacement makes a new snapshot.
    pub fn same_snapshot(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where a game reads the resident scene textures of the frame in flight.
///
/// Every route publishes them before any game feature prepares: with the lent terrain in the
/// frame, and also inside a depth prefix that a route submits ahead of the frame, because the
/// textures are resident scene content rather than a product of the frame, so a depth-prefix
/// feature may read them. The textures are the renderer's own: sample them, never write them,
/// and take them again each frame.
#[derive(Clone, Debug, Default)]
pub struct FrameSceneTexturesHandle(Lent<FrameSceneTextures>);

impl FrameSceneTexturesHandle {
    /// The textures of the frame now being prepared.
    ///
    /// # Errors
    /// [`FrameInputError::OutsideFrame`] outside a frame and [`FrameInputError::NotPrepared`]
    /// before the route has published them.
    pub fn current(&self) -> Result<FrameSceneTextures, FrameInputError> {
        self.0.current()
    }

    pub(crate) fn publish(&self, textures: impl FnOnce() -> FrameSceneTextures) {
        self.0.publish_with(textures);
    }
}

impl From<FrameSceneTextureError> for worldrunner_render_post::FrameError {
    fn from(error: FrameSceneTextureError) -> Self {
        Self::FeaturePreparation {
            feature: "frame scene textures",
            reason: error.to_string(),
        }
    }
}

impl crate::compositor::FrameRenderer {
    /// Lends `gpu`'s resident textures to the game's features, when there are any. Every route
    /// calls it before features prepare, and the depth prefix calls it before its game passes.
    pub(crate) fn publish_game_textures(&self, gpu: &crate::draw::SceneGpu) {
        if self.game.features > 0 {
            self.game_textures
                .publish(|| FrameSceneTextures::new(gpu.lent_textures()));
        }
    }
}
