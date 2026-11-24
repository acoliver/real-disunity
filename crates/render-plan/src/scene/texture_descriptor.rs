//! Source-free descriptors of validated textures.
//!
//! A [`SceneTextureDescriptor`] keeps what a consumer of the texture table needs after the
//! source bytes are gone: formats, extents, mip counts, colour spaces, cube layout and the
//! exact length of the payload it replaced. Container facts such as a `DDS` header can only
//! be established by the renderer's parser, so `worldrunner-render-scene` builds those
//! descriptors; this module validates them and answers table queries.

use super::{
    SceneResources, SceneTexture, SceneValidationError, TexelFormat, TextureColorSpace,
    validate_extent, validate_texel_layout,
};
use crate::scene::{SceneSpecularCube, SceneSpecularCubeFormat, SceneSpecularPreparation};

/// Block-compressed `DDS` formats a descriptor can record.
///
/// The colour space is recorded beside the format, because several formats exist in a
/// linear and an sRGB variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneDdsFormat {
    /// Four-channel BC1 with 8-byte blocks.
    Bc1Rgba,
    /// Four-channel BC3 with 16-byte blocks.
    Bc3Rgba,
    /// One-channel BC4 with 8-byte blocks; linear only.
    Bc4R,
    /// Two-channel BC5 with 16-byte blocks; linear only.
    Bc5Rg,
    /// Four-channel BC7 with 16-byte blocks.
    Bc7Rgba,
}

impl SceneDdsFormat {
    /// Returns the byte length of one 4x4 block.
    #[must_use]
    pub const fn block_bytes(self) -> u64 {
        match self {
            Self::Bc1Rgba | Self::Bc4R => 8,
            Self::Bc3Rgba | Self::Bc5Rg | Self::Bc7Rgba => 16,
        }
    }

    /// Returns the number of channels the format stores.
    #[must_use]
    pub const fn channel_count(self) -> u32 {
        match self {
            Self::Bc4R => 1,
            Self::Bc5Rg => 2,
            Self::Bc1Rgba | Self::Bc3Rgba | Self::Bc7Rgba => 4,
        }
    }

    /// Returns whether the format can store the declared colour space.
    #[must_use]
    pub const fn admits(self, color_space: TextureColorSpace) -> bool {
        match self {
            Self::Bc4R | Self::Bc5Rg => matches!(color_space, TextureColorSpace::Linear),
            Self::Bc1Rgba | Self::Bc3Rgba | Self::Bc7Rgba => true,
        }
    }

    /// Returns the byte length of a tightly packed mip chain, or `None` on overflow.
    fn chain_bytes(self, width: u32, height: u32, mips: u32) -> Option<u64> {
        (0..mips).try_fold(0_u64, |total, level| {
            let blocks = u64::from((width >> level).max(1).div_ceil(4))
                .checked_mul(u64::from((height >> level).max(1).div_ceil(4)))?;
            total.checked_add(blocks.checked_mul(self.block_bytes())?)
        })
    }
}

/// Byte length of the magic number, header and `DX10` extension that precede the mip chain.
const DDS_HEADER_BYTES: u64 = 148;

/// Layout and accounting of an explicit texel grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTexelDescriptor {
    /// Closed texel format of every level.
    pub format: TexelFormat,
    /// Colour space the stored values are authored in.
    pub color_space: TextureColorSpace,
    /// Texel width of the base level.
    pub width: u32,
    /// Texel height of the base level.
    pub height: u32,
    /// Whole mip levels including the base level.
    pub mips: u32,
    /// Length of the released payload in bytes.
    pub logical_bytes: u64,
}

impl SceneTexelDescriptor {
    fn validate(&self) -> Result<(), SceneValidationError> {
        validate_texel_layout(
            self.format,
            self.width,
            self.height,
            self.mips,
            self.logical_bytes,
        )?;
        // Only the 8-bit four-channel formats have an sRGB GPU variant.
        let srgb_format = matches!(
            self.format,
            TexelFormat::Rgba8Unorm | TexelFormat::Bgra8Unorm
        );
        if self.color_space == TextureColorSpace::Srgb && !srgb_format {
            return Err(SceneValidationError::UnsupportedInput {
                part: "released texel format cannot store the declared color space",
            });
        }
        isize::try_from(self.logical_bytes).map_err(|_| {
            SceneValidationError::ArithmeticOverflow {
                part: "released texel texture",
            }
        })?;
        Ok(())
    }
}

/// Header facts of one validated `DDS` container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDdsDescriptor {
    /// Block format the container stores.
    pub format: SceneDdsFormat,
    /// Colour space the scene declared; it agrees with the stored format.
    pub color_space: TextureColorSpace,
    /// Texel width of the base level.
    pub width: u32,
    /// Texel height of the base level.
    pub height: u32,
    /// Mip levels the container declares and holds.
    pub mips: u32,
    /// Length of the whole container in bytes, including the header and any trailing bytes.
    pub logical_bytes: u64,
}

/// Layout, preparation request and accounting of one specular cube.
///
/// The released payload was six faces in the order +X, -X, +Y, -Y, +Z, -Z, each holding its mips
/// from largest to smallest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCubeDescriptor {
    /// Texel format of every face level.
    pub format: SceneSpecularCubeFormat,
    /// Edge length of the base level of one face.
    pub size: u32,
    /// Mip levels held per face.
    pub mips: u32,
    /// Length of the released payload in bytes.
    pub logical_bytes: u64,
    /// The host-side filtering request the source carried, if any.
    pub preparation: Option<SceneSpecularPreparation>,
}

impl SceneCubeDescriptor {
    fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "released specular cube";
        validate_extent(PART, self.size, self.size, self.mips)?;
        let pow2 = self.size.is_power_of_two() && self.size <= 1024;
        let block_aligned = self.format == SceneSpecularCubeFormat::Rgba16Float
            || (self.size >= 4 && self.mips == self.size.ilog2() + 1);
        if !pow2 || !block_aligned {
            return Err(SceneValidationError::UnsupportedInput {
                part: "released specular cube extent or mips",
            });
        }
        if let Some(request) = self.preparation {
            request.validate()?;
        }
        let expected =
            crate::scene::specular_cube::payload_bytes(self.format, self.size, self.mips);
        if self.logical_bytes != expected {
            return Err(SceneValidationError::ByteLengthMismatch {
                part: PART,
                reason: "cube payload must equal six faces of concatenated level sizes",
                expected,
                observed: self.logical_bytes,
            });
        }
        Ok(())
    }
}

impl SceneDdsDescriptor {
    fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "released dds texture";
        validate_extent(PART, self.width, self.height, self.mips)?;
        if !self.format.admits(self.color_space) {
            return Err(SceneValidationError::UnsupportedInput {
                part: "released dds format cannot store the declared color space",
            });
        }
        let overflow = SceneValidationError::ArithmeticOverflow { part: PART };
        let minimum = self
            .format
            .chain_bytes(self.width, self.height, self.mips)
            .and_then(|chain| chain.checked_add(DDS_HEADER_BYTES))
            .ok_or_else(|| overflow.clone())?;
        if self.logical_bytes < minimum {
            return Err(SceneValidationError::ByteLengthMismatch {
                part: PART,
                reason: "container must hold the 148-byte header and the declared mip chain",
                expected: minimum,
                observed: self.logical_bytes,
            });
        }
        // A `Vec` or `Box` source cannot hold more than `isize::MAX` bytes, so a longer
        // logical length describes no payload that could have been released.
        if isize::try_from(self.logical_bytes).is_err() {
            return Err(overflow);
        }
        Ok(())
    }
}

/// Payload-free description of one validated texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneTextureDescriptor {
    /// An explicit texel grid.
    Texels(SceneTexelDescriptor),
    /// A validated `DDS` container.
    Dds(SceneDdsDescriptor),
    /// A specular cube.
    SpecularCube(SceneCubeDescriptor),
}

impl SceneTextureDescriptor {
    /// Returns the declared colour space; cubes hold linear radiance.
    #[must_use]
    pub const fn color_space(&self) -> TextureColorSpace {
        match self {
            Self::Texels(texels) => texels.color_space,
            Self::Dds(dds) => dds.color_space,
            Self::SpecularCube(_) => TextureColorSpace::Linear,
        }
    }

    /// Returns the byte length of the source payload this descriptor replaced.
    #[must_use]
    pub const fn logical_bytes(&self) -> u64 {
        match self {
            Self::Texels(texels) => texels.logical_bytes,
            Self::Dds(dds) => dds.logical_bytes,
            Self::SpecularCube(cube) => cube.logical_bytes,
        }
    }

    /// Validates the descriptor with the arithmetic the source payload satisfied.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when an extent is zero, a mip count exceeds the
    /// addressable levels, a format cannot store the colour space, or the byte length
    /// contradicts the layout.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        match self {
            Self::Texels(texels) => texels.validate(),
            Self::Dds(dds) => dds.validate(),
            Self::SpecularCube(cube) => cube.validate(),
        }
    }
}

impl SceneSpecularCube {
    /// Describes this cube without its payload.
    #[must_use]
    pub fn descriptor(&self) -> SceneCubeDescriptor {
        SceneCubeDescriptor {
            format: self.format(),
            size: self.size(),
            mips: self.mip_count(),
            logical_bytes: self.bytes().len() as u64,
            preparation: self.preparation(),
        }
    }
}

impl SceneTexture {
    /// Describes this texture when no container needs parsing.
    ///
    /// Texel grids, specular cubes and released textures answer; an encoded container
    /// returns `None` because only the renderer's container reader can describe it.
    #[must_use]
    pub fn domain_descriptor(&self) -> Option<SceneTextureDescriptor> {
        match self.payload() {
            Self::SpecularCube(cube) => {
                Some(SceneTextureDescriptor::SpecularCube(cube.descriptor()))
            }
            Self::Texels {
                format,
                color_space,
                width,
                height,
                mips,
                bytes,
            } => Some(SceneTextureDescriptor::Texels(SceneTexelDescriptor {
                format: *format,
                color_space: *color_space,
                width: *width,
                height: *height,
                mips: *mips,
                logical_bytes: bytes.len() as u64,
            })),
            Self::Released(descriptor) => Some(*descriptor),
            Self::Encoded { .. } | Self::Sampled { .. } => None,
        }
    }

    /// Returns whether this texture is a specular cube, released or not.
    #[must_use]
    pub fn is_specular_cube(&self) -> bool {
        matches!(
            self.payload(),
            Self::SpecularCube(_) | Self::Released(SceneTextureDescriptor::SpecularCube(_))
        )
    }

    /// Returns the byte length of the source payload, whether or not it is still held.
    #[must_use]
    pub fn logical_source_bytes(&self) -> u64 {
        match self {
            Self::Sampled { texture, .. } => texture.logical_source_bytes(),
            Self::SpecularCube(cube) => cube.bytes().len() as u64,
            Self::Texels { bytes, .. } | Self::Encoded { bytes, .. } => bytes.len() as u64,
            Self::Released(descriptor) => descriptor.logical_bytes(),
        }
    }

    /// Returns the source payload bytes this value still holds; zero once released.
    #[must_use]
    pub fn retained_source_bytes(&self) -> u64 {
        match self {
            Self::Sampled { texture, .. } => texture.retained_source_bytes(),
            Self::Released(_) => 0,
            Self::SpecularCube(_) | Self::Texels { .. } | Self::Encoded { .. } => {
                self.logical_source_bytes()
            }
        }
    }
}

/// Sums the logical source bytes of `textures`, failing when the sum leaves `u64`.
pub(super) fn logical_total(textures: &[SceneTexture]) -> Result<u64, SceneValidationError> {
    textures
        .iter()
        .try_fold(0_u64, |total, texture| {
            total.checked_add(texture.logical_source_bytes())
        })
        .ok_or_else(total_overflow)
}

fn total_overflow() -> SceneValidationError {
    SceneValidationError::ArithmeticOverflow {
        part: "texture table logical bytes",
    }
}

impl SceneResources {
    /// Returns the byte length of every texture source payload, released or not.
    ///
    /// The table stores this checked sum when it adopts, appends or replaces textures, so
    /// the query does not walk the table.
    #[must_use]
    pub const fn logical_texture_bytes(&self) -> u64 {
        self.logical_bytes
    }

    /// Returns the cached total with `added` appended, without publishing it.
    pub(super) fn total_with(&self, added: &[SceneTexture]) -> Result<u64, SceneValidationError> {
        self.logical_bytes
            .checked_add(logical_total(added)?)
            .ok_or_else(total_overflow)
    }

    /// Returns the cached total with the texture at `index` replaced, without publishing it.
    pub(super) fn total_replacing(
        &self,
        index: usize,
        replacement: &SceneTexture,
    ) -> Result<u64, SceneValidationError> {
        self.logical_bytes
            .checked_sub(self.textures[index].logical_source_bytes())
            .and_then(|rest| rest.checked_add(replacement.logical_source_bytes()))
            .ok_or_else(total_overflow)
    }

    /// Returns the source payload bytes this table value can still reach.
    ///
    /// This counts payload lengths reachable through this value. It is not an allocation
    /// measurement: a clone that shares the table keeps the same bytes alive. Each entry
    /// retains at most its logical length, so the sum is bounded by
    /// [`Self::logical_texture_bytes`] and cannot overflow.
    #[must_use]
    pub fn retained_texture_source_bytes(&self) -> u64 {
        self.textures
            .iter()
            .map(SceneTexture::retained_source_bytes)
            .sum()
    }
}
