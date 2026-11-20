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
    #[error("scene texture {handle} is not two-dimensional, so it has no 2D array view")]
    NotTwoDimensional { handle: u32 },
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
        let texture = self.texture(handle)?;
        Ok(Import::texture(
            semantic,
            texture.clone(),
            texture.create_view(&wgpu::TextureViewDescriptor::default()),
        ))
    }

    /// The texture of `handle` viewed as a 2D array of all its layers and mips, for a binding
    /// the shader declares as a 2D texture array (HLSL `Texture2DArray`, WGSL
    /// `texture_2d_array`). A one-layer scene texture is a one-layer array, which the program
    /// reads as layer 0.
    ///
    /// # Errors
    /// [`FrameSceneTextureError::Unknown`] for a handle beyond the resident table and
    /// [`FrameSceneTextureError::NotTwoDimensional`] for a texture whose dimension is not 2D.
    pub fn import_array(
        &self,
        semantic: EngineSemantic,
        handle: TextureHandle,
    ) -> Result<Import, FrameSceneTextureError> {
        let texture = self.texture(handle)?;
        if texture.dimension() != wgpu::TextureDimension::D2 {
            return Err(FrameSceneTextureError::NotTwoDimensional {
                handle: handle.raw(),
            });
        }
        Ok(Import::texture_array(
            semantic,
            texture.clone(),
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
        ))
    }

    fn texture(&self, handle: TextureHandle) -> Result<&wgpu::Texture, FrameSceneTextureError> {
        usize::try_from(handle.raw())
            .ok()
            .and_then(|index| self.textures.get(index))
            .ok_or(FrameSceneTextureError::Unknown {
                handle: handle.raw(),
                resident: self.textures.len(),
            })
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

#[cfg(test)]
mod tests {
    use super::{FrameSceneTextureError, FrameSceneTextures};
    use crate::device::GpuContext;
    use crate::test_support::SceneTestSession;
    use worldrunner_render_plan::TextureHandle;
    use worldrunner_render_program::binding::EngineSemantic;

    fn texture(context: &GpuContext, dimension: wgpu::TextureDimension) -> wgpu::Texture {
        context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("scene texture test"),
            size: wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    #[test]
    fn an_array_view_needs_a_two_dimensional_texture_in_the_table() {
        let _session = SceneTestSession::acquire();
        let context =
            GpuContext::shared_for_test_with_features(wgpu::Features::empty()).expect("a test GPU");
        let textures = FrameSceneTextures::new(
            [
                texture(&context, wgpu::TextureDimension::D2),
                texture(&context, wgpu::TextureDimension::D3),
            ]
            .into(),
            super::Rc::from([]),
        );
        let semantic = EngineSemantic::named("scene");
        assert!(
            textures
                .import_array(semantic.clone(), TextureHandle::from_raw(0))
                .is_ok()
        );
        assert_eq!(
            textures
                .import_array(semantic.clone(), TextureHandle::from_raw(1))
                .err(),
            Some(FrameSceneTextureError::NotTwoDimensional { handle: 1 })
        );
        assert_eq!(
            textures
                .import_array(semantic, TextureHandle::from_raw(2))
                .err(),
            Some(FrameSceneTextureError::Unknown {
                handle: 2,
                resident: 2
            })
        );
    }
}
