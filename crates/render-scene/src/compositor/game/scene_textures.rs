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
pub struct FrameSceneTextures {
    textures: Rc<[wgpu::Texture]>,
    /// The normal map of each water body the renderer has installed.
    water_normals: Rc<[Option<TextureHandle>]>,
}

/// Why the installed water's normal map cannot be lent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameWaterNormalError {
    #[error("the renderer has no water installed")]
    NoWater,
    #[error("water body {body} is not among the {bodies} installed")]
    UnknownBody { body: usize, bodies: usize },
    #[error("water body {body} has no normal map")]
    NoNormalMap { body: usize },
    #[error(transparent)]
    Texture(#[from] FrameSceneTextureError),
}

impl FrameSceneTextures {
    pub(crate) fn new(
        textures: Rc<[wgpu::Texture]>,
        water_normals: Rc<[Option<TextureHandle>]>,
    ) -> Self {
        Self {
            textures,
            water_normals,
        }
    }

    /// The normal map of installed water body `body`, for the binding sourced from `semantic`:
    /// the resident texture the body's settings name, through its default view. It is read from
    /// the water settings the renderer was configured with, so it does not depend on whether
    /// the stock passes draw the water.
    ///
    /// # Errors
    /// [`FrameWaterNormalError::NoWater`] when the renderer has no water installed,
    /// [`FrameWaterNormalError::UnknownBody`] beyond the installed bodies and
    /// [`FrameWaterNormalError::NoNormalMap`] for a body without a normal map.
    pub fn water_normal_map(
        &self,
        semantic: EngineSemantic,
        body: usize,
    ) -> Result<Import, FrameWaterNormalError> {
        let bodies = self.water_normals.len();
        if bodies == 0 {
            return Err(FrameWaterNormalError::NoWater);
        }
        let handle = self
            .water_normals
            .get(body)
            .ok_or(FrameWaterNormalError::UnknownBody { body, bodies })?
            .ok_or(FrameWaterNormalError::NoNormalMap { body })?;
        Ok(self.import(semantic, handle)?)
    }

    /// Textures in the resident table.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
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
            .and_then(|index| self.textures.get(index))
            .ok_or(FrameSceneTextureError::Unknown {
                handle: handle.raw(),
                resident: self.textures.len(),
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
        Rc::ptr_eq(&self.textures, &other.textures)
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
    /// Lends `gpu`'s resident textures and meshes to the game's features, when there are any.
    /// Every route calls it before features prepare, and the depth prefix calls it before its
    /// game passes.
    pub(crate) fn publish_game_scene(&self, gpu: &crate::draw::SceneGpu) {
        if self.game.features > 0 {
            self.game_textures
                .publish(|| FrameSceneTextures::new(gpu.lent_textures(), self.water_normals()));
            self.game
                .meshes
                .publish(|| super::FrameSceneMeshes::new(gpu.lent_meshes()));
            if let Some(engine) = &self.game.engine {
                engine.publish_scene(super::SceneSources {
                    textures: gpu.lent_textures(),
                    terrain: gpu.lent_terrain(),
                });
            }
        }
    }

    /// The normal map handle of each water body installed on this renderer.
    fn water_normals(&self) -> Rc<[Option<TextureHandle>]> {
        self.water.as_ref().map_or_else(
            || Rc::from([]),
            |water| {
                water
                    .settings
                    .iter()
                    .map(|body| body.material.normal)
                    .collect()
            },
        )
    }
}
