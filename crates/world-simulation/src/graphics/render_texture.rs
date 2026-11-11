//! Render textures: the surfaces cameras and blits draw into.
use super::{GraphicsError, RenderTextureId};
use crate::{World, WorldError};

/// Largest width or height of a render texture.
const MAX_SIZE: u32 = 16384;
/// Most mip levels of a render texture: the full chain of the largest size.
const MAX_MIPS: u32 = 15;
/// Most slices of a volume, layers of an array or faces of a cube array.
pub const MAX_VOLUME_DEPTH: u32 = 2048;

/// The engine's `TextureDimension` values a render texture can have.
pub mod dimension {
    pub const TEX_2D: u32 = 2;
    pub const TEX_3D: u32 = 3;
    pub const CUBE: u32 = 4;
    pub const TEX_2D_ARRAY: u32 = 5;
    pub const CUBE_ARRAY: u32 = 6;
}

/// Highest wrap mode value.
const MAX_WRAP: u32 = 3;
/// Highest filter mode value.
const MAX_FILTER: u32 = 2;
/// Highest anisotropy level.
const MAX_ANISO: u32 = 16;

/// How a render texture is sampled. `wrap_*` are the engine's `TextureWrapMode` (0 repeat,
/// 1 clamp, 2 mirror, 3 mirror once) and `filter` its `FilterMode` (0 point, 1 bilinear,
/// 2 trilinear). The default is the engine's choice for a texture nothing configured: repeat,
/// bilinear, anisotropy 1 and no bias.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldSampler {
    pub wrap_u: u32,
    pub wrap_v: u32,
    pub wrap_w: u32,
    pub filter: u32,
    pub aniso_level: u32,
    pub mip_bias: f32,
}

impl Default for WorldSampler {
    fn default() -> Self {
        Self {
            wrap_u: 0,
            wrap_v: 0,
            wrap_w: 0,
            filter: 1,
            aniso_level: 1,
            mip_bias: 0.0,
        }
    }
}

impl WorldSampler {
    fn is_valid(&self) -> bool {
        [self.wrap_u, self.wrap_v, self.wrap_w]
            .iter()
            .all(|wrap| *wrap <= MAX_WRAP)
            && self.filter <= MAX_FILTER
            && self.aniso_level <= MAX_ANISO
            && self.mip_bias.is_finite()
    }
}

/// A render texture's description. `format` is the engine's `GraphicsFormat` value. Every edit,
/// including a sampler change, raises `generation`; the description a caller passes in only
/// chooses the other fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRenderTexture {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    /// Bits of the depth buffer: 0, 16, 24 or 32.
    pub depth_bits: u32,
    pub mip_count: u32,
    pub srgb: bool,
    /// The engine's `TextureDimension`: 2 (2D), 3 (3D), 4 (cube), 5 (2D array) or 6 (cube
    /// array); see [`dimension`].
    pub dimension: u32,
    /// Slices of a 3D texture, layers of an array, six faces per cube of a cube array; 1 for a
    /// 2D texture or a cube.
    pub volume_depth: u32,
    pub sampler: WorldSampler,
    pub generation: u64,
}

impl WorldRenderTexture {
    pub(super) fn validate(&self) -> Result<(), GraphicsError> {
        let sized = (1..=MAX_SIZE).contains(&self.width) && (1..=MAX_SIZE).contains(&self.height);
        let depth = matches!(self.depth_bits, 0 | 16 | 24 | 32);
        let mips = (1..=MAX_MIPS).contains(&self.mip_count);
        if sized && depth && mips && self.shape_is_valid() && self.sampler.is_valid() {
            Ok(())
        } else {
            Err(GraphicsError::InvalidRenderTexture)
        }
    }
}

impl WorldRenderTexture {
    /// A 2D texture or a cube has one slice and a cube is square; a 3D texture or an array has
    /// 1 to [`MAX_VOLUME_DEPTH`]; a cube array is square and has whole cubes of six faces.
    fn shape_is_valid(&self) -> bool {
        let deep = (1..=MAX_VOLUME_DEPTH).contains(&self.volume_depth);
        let square = self.width == self.height;
        match self.dimension {
            dimension::TEX_2D => self.volume_depth == 1,
            dimension::CUBE => self.volume_depth == 1 && square,
            dimension::TEX_3D | dimension::TEX_2D_ARRAY => deep,
            dimension::CUBE_ARRAY => deep && square && self.volume_depth.is_multiple_of(6),
            _ => false,
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
    /// When a record of the frame's offscreen work names it, the frame's snapshot keeps its
    /// description (see [`crate::graphics::GraphicsSnapshot::take`]); new work cannot name it.
    pub fn destroy_render_texture(&mut self, id: RenderTextureId) -> Result<(), WorldError> {
        let held = self
            .graphics
            .render_textures
            .remove(id.0)
            .ok_or(WorldError::Graphics(GraphicsError::StaleRenderTexture))?;
        if self
            .graphics
            .offscreen
            .iter()
            .any(|work| work.names_render_texture(id))
        {
            self.graphics.retired_textures.insert(id, held);
        }
        Ok(())
    }

    /// Takes the descriptions of the render textures destroyed after a record of the frame named
    /// them.
    pub(crate) fn take_retired_render_textures(
        &mut self,
    ) -> std::collections::BTreeMap<RenderTextureId, WorldRenderTexture> {
        std::mem::take(&mut self.graphics.retired_textures)
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
