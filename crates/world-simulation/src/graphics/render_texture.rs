//! Render textures: the surfaces cameras and blits draw into.
use super::{GraphicsError, RenderTextureId};
use crate::{World, WorldError};

/// Largest width or height of a render texture.
const MAX_SIZE: u32 = 16384;
/// Most mip levels of a render texture: the full chain of the largest size.
const MAX_MIPS: u32 = 15;

/// A render texture's description. `format` is the engine's `GraphicsFormat` value. Every edit
/// raises `generation`; the description a caller passes in only chooses the other fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRenderTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    /// Bits of the depth buffer: 0, 16, 24 or 32.
    pub depth_bits: u32,
    pub mip_count: u32,
    pub srgb: bool,
    pub generation: u64,
}

impl WorldRenderTexture {
    fn validate(&self) -> Result<(), GraphicsError> {
        let sized = (1..=MAX_SIZE).contains(&self.width) && (1..=MAX_SIZE).contains(&self.height);
        let depth = matches!(self.depth_bits, 0 | 16 | 24 | 32);
        if sized && depth && (1..=MAX_MIPS).contains(&self.mip_count) {
            Ok(())
        } else {
            Err(GraphicsError::InvalidRenderTexture)
        }
    }
}

impl World {
    /// Creates a render texture from a description. The generation of `texture` is ignored: a new
    /// texture starts at 1.
    pub fn create_render_texture(
        &mut self,
        texture: WorldRenderTexture,
    ) -> Result<RenderTextureId, WorldError> {
        texture.validate()?;
        let texture = WorldRenderTexture {
            generation: 1,
            ..texture
        };
        Ok(RenderTextureId(
            self.graphics.render_textures.insert(texture)?,
        ))
    }

    /// Replaces a render texture's description and raises its generation by one.
    pub fn update_render_texture(
        &mut self,
        id: RenderTextureId,
        texture: WorldRenderTexture,
    ) -> Result<(), WorldError> {
        texture.validate()?;
        let held = self
            .graphics
            .render_textures
            .get_mut(id.0)
            .ok_or(GraphicsError::StaleRenderTexture)?;
        *held = WorldRenderTexture {
            generation: held.generation + 1,
            ..texture
        };
        Ok(())
    }

    /// Removes a render texture. Whatever names it goes stale; see the module documentation.
    pub fn destroy_render_texture(&mut self, id: RenderTextureId) -> Result<(), WorldError> {
        self.graphics
            .render_textures
            .remove(id.0)
            .map(drop)
            .ok_or(WorldError::Graphics(GraphicsError::StaleRenderTexture))
    }

    pub fn render_texture(&self, id: RenderTextureId) -> Result<&WorldRenderTexture, WorldError> {
        Ok(self
            .graphics
            .render_textures
            .get(id.0)
            .ok_or(GraphicsError::StaleRenderTexture)?)
    }

    /// Every live render texture, in slot order.
    pub fn render_textures(&self) -> impl Iterator<Item = (RenderTextureId, &WorldRenderTexture)> {
        self.graphics
            .render_textures
            .iter()
            .map(|(bits, texture)| (RenderTextureId(bits), texture))
    }
}
