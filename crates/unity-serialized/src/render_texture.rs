//! The layout of `RenderTexture` objects, the textures a camera or script draws into.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader},
};

/// A render texture's allocation settings and sampler. The formats are `GraphicsFormat` values:
/// `depth_stencil_format` 0 means no depth buffer. Nothing else the object stores is decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderTexture {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub anti_aliasing: i32,
    /// The levels of the chain; -1 asks for the full chain.
    pub mip_count: i32,
    pub depth_stencil_format: i32,
    pub color_format: i32,
    pub mip_map: bool,
    pub generate_mips: bool,
    pub filter_mode: i32,
    pub aniso: i32,
    pub mip_bias: f32,
    /// Wrap modes for U, V and W.
    pub wrap: [i32; 3],
    /// A `TextureDimension` value; 2 is a 2D texture.
    pub dimension: i32,
    pub volume_depth: i32,
    /// A `ShadowSamplingMode` value; 2 is none.
    pub shadow_sampling: i32,
}

impl Layout for RenderTexture {
    const CLASS: ClassId = ClassId::RENDER_TEXTURE;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        r.align()?;
        // A 32-bit field precedes the size; it is zero in every Old World render texture.
        r.i32()?;
        let [width, height, anti_aliasing, mip_count] = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
        let [depth_stencil_format, color_format] = [r.i32()?, r.i32()?];
        let [mip_map, generate_mips] = [r.bool()?, r.bool()?];
        // Six more flag bytes follow; the planner does not use them.
        r.bytes::<6>()?;
        r.align()?;
        Ok(Self {
            name,
            width,
            height,
            anti_aliasing,
            mip_count,
            depth_stencil_format,
            color_format,
            mip_map,
            generate_mips,
            filter_mode: r.i32()?,
            aniso: r.i32()?,
            mip_bias: r.f32()?,
            wrap: [r.i32()?, r.i32()?, r.i32()?],
            dimension: r.i32()?,
            volume_depth: r.i32()?,
            shadow_sampling: r.i32()?,
        })
    }
}
