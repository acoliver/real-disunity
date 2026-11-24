//! A stored 2D array or cube as render-program's source array or cube.
//!
//! The byte string holds `depth` layers (the faces of a cube in the order +X, -X, +Y, -Y, +Z, -Z)
//! of one length, each holding the levels of a 2D texture one after the other, base level first,
//! as the engine stores them.

use worldrunner_disunity_runtime::{PlayerTexture, TextureDimension};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::SamplerState;
use worldrunner_render_program::unity::offscreen::frame::{SourceDimension, SourceTexture};

use super::assets::{
    BC6H, BC6H_UFLOAT, DXT1_LINEAR, DXT1_SRGB, DXT5_LINEAR, DXT5_SRGB, RGBA8_LINEAR, RGBA8_SRGB,
    block_upload,
};
use crate::textures::{PlayerTextureError, player_rgba8};

/// The engine's `TextureFormat`s a 2D array, cube or volume is read in as texels: Alpha8, RGB24,
/// RGBA32, ARGB32 and BGRA32.
const ARRAY_FORMATS: [u32; 5] = [1, 3, 4, 5, 14];
/// The `GraphicsFormat` of the terrain atlases' normal, metallic and roughness maps. It has no
/// `TextureFormat` and stores four bytes a texel, blue first: byte 0 is blue, byte 1 green and
/// byte 2 red, and the alpha byte is 0 in Old World's atlases. Bytes 1 and 2 sit near 187, the
/// sRGB encoding of 0.5, which is where a normal's x and y are neutral. It goes to the device as
/// sRGB RGBA8 with bytes 0 and 2 exchanged, by [`red_first`], on every path that uploads it: 2D
/// arrays, cubes and volumes. A plain 2D texture has no `TextureFormat` for it and is refused.
pub(super) const FOUR_BYTE_ARRAY: u32 = 87;
/// The `GraphicsFormat` of a four-byte data volume, such as a noise volume. It has no
/// `TextureFormat` and stores four bytes a texel. It goes to the device as the RGBA8 whose layout
/// those bytes have, in the colour space the record states: a data volume states linear.
pub(super) const FOUR_BYTE_DATA: u32 = 88;

/// How the layers of a stored array go to the device as they are stored, every level of every
/// layer.
struct Verbatim {
    /// The `GraphicsFormat` they are uploaded as.
    graphics_format: u32,
    /// The side of a block in texels, 4 for DXT1 and DXT5 blocks and 1 for texels.
    side: u32,
    /// The bytes of a block or texel.
    bytes: usize,
    /// The name of the format, for the refusal.
    name: &'static str,
    /// Whether a texel stores blue, green, red, alpha and goes up as red, green, blue, alpha.
    blue_first: bool,
}

impl Verbatim {
    /// The layout of an array or cube of `stored`, when its layers are uploaded as they are. The
    /// record's `graphics_format` decides when it names one: DXT1 and DXT5 in either colour space,
    /// and the four-byte 87 and 88 the engine has no `TextureFormat` for. A record without one, or
    /// with another, falls back to its `TextureFormat`: DXT1 and DXT5 follow `linear`, and a cube
    /// in BC6H goes up as the blocks of an unsigned-float texture.
    fn of(stored: &PlayerTexture, shape: Layered) -> Option<Self> {
        if shape == Layered::Cube && stored.format == BC6H {
            return Some(Self {
                graphics_format: BC6H_UFLOAT,
                side: BLOCK_SIDE,
                bytes: 16,
                name: "BC6H",
                blue_first: false,
            });
        }
        let by_graphics_format = match stored.graphics_format {
            FOUR_BYTE_ARRAY => Some((RGBA8_SRGB, 1, 4, "GraphicsFormat 87", true)),
            FOUR_BYTE_DATA => Some((four_byte_data(stored), 1, 4, "GraphicsFormat 88", false)),
            format @ (DXT1_SRGB | DXT1_LINEAR) => Some((format, BLOCK_SIDE, 8, "DXT1", false)),
            format @ (DXT5_SRGB | DXT5_LINEAR) => Some((format, BLOCK_SIDE, 16, "DXT5", false)),
            _ => None,
        };
        if let Some((graphics_format, side, bytes, name, blue_first)) = by_graphics_format {
            return Some(Self {
                graphics_format,
                side,
                bytes,
                name,
                blue_first,
            });
        }
        let (graphics_format, bytes, name) = block_upload(stored)?;
        matches!(stored.format, 10 | 12).then_some(Self {
            graphics_format,
            side: BLOCK_SIDE,
            bytes,
            name,
            blue_first: false,
        })
    }

    /// The bytes of one layer's `mips` levels, each at least one block.
    fn layer_bytes(&self, stored: &PlayerTexture, mips: u32) -> usize {
        let blocks = |side: u32, level: u32| {
            side.checked_shr(level)
                .unwrap_or(0)
                .max(1)
                .div_ceil(self.side) as usize
        };
        (0..mips)
            .map(|level| blocks(stored.width, level) * blocks(stored.height, level) * self.bytes)
            .sum()
    }
}

/// The side of a block of a block-compressed format, in texels.
const BLOCK_SIDE: u32 = 4;

/// The RGBA8 `GraphicsFormat` a [`FOUR_BYTE_DATA`] texture goes up as: in the record's colour space.
pub(super) fn four_byte_data(stored: &PlayerTexture) -> u32 {
    if stored.linear {
        RGBA8_LINEAR
    } else {
        RGBA8_SRGB
    }
}

/// A texture of layers: the layers of a 2D array or the six faces of a cube.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layered {
    Array,
    Cube,
}

impl Layered {
    /// What the texture is called in a refusal.
    fn noun(self) -> &'static str {
        match self {
            Self::Array => "array",
            Self::Cube => "cube",
        }
    }

    fn source(self, depth: u32) -> SourceDimension {
        match self {
            Self::Array => SourceDimension::D2Array(depth),
            Self::Cube => SourceDimension::Cube,
        }
    }

    /// Refuses a cube that does not have six square faces, which the record's validation also
    /// refuses.
    fn check(self, stored: &PlayerTexture) -> Result<(), PlayerTextureError> {
        if self == Self::Cube && (stored.depth != CUBE_FACES || stored.width != stored.height) {
            return Err(PlayerTextureError::CubeFaces {
                depth: stored.depth,
                width: stored.width,
                height: stored.height,
            });
        }
        Ok(())
    }
}

/// The faces of a cube.
const CUBE_FACES: u32 = 6;

/// A stored 2D array or cube as a source array or cube. The byte string holds `depth` layers (the
/// faces of a cube in the order +X, -X, +Y, -Y, +Z, -Z) of one length, each holding the `mips`
/// levels of a 2D texture one after the other, base level first, as the engine stores them. DXT1
/// and DXT5 layers go up as the blocks they are, and only to a device that samples blocks, as
/// layers are not decoded; the four-byte `GraphicsFormat` 87 goes up as the texels it holds, with
/// its blue and red bytes exchanged.
/// Alpha8, RGB24 and RGBA32 levels are read as RGBA8 as a plain texture is. Every way leaves the
/// layers and levels in the order the engine stored them.
pub(super) fn layers_of(
    id: TextureId,
    stored: &PlayerTexture,
    sampler: Option<SamplerState>,
    blocks: bool,
    shape: Layered,
) -> Result<SourceTexture, PlayerTextureError> {
    shape.check(stored)?;
    let layers = stored.depth as usize;
    let length = stored.bytes.len();
    if length == 0 || !length.is_multiple_of(layers) {
        return Err(PlayerTextureError::Layers {
            depth: stored.depth,
            actual: length,
        });
    }
    let mips = stored.mips.max(1);
    check_levels(stored, mips)?;
    let (graphics_format, bytes) = match Verbatim::of(stored, shape) {
        Some(layout) => (
            layout.graphics_format,
            verbatim_layers(stored, &layout, blocks)?,
        ),
        None => (
            texel_format(stored, shape.noun())?,
            rgba8_layers(stored, mips)?,
        ),
    };
    Ok(SourceTexture {
        id,
        dimension: shape.source(stored.depth),
        size: [stored.width, stored.height],
        graphics_format,
        mips,
        bytes,
        sampler,
    })
}

/// Refuses more levels than the base size has, before anything is sized by the level count.
pub(super) fn check_levels(stored: &PlayerTexture, mips: u32) -> Result<(), PlayerTextureError> {
    let most = u32::BITS - stored.width.max(stored.height).leading_zeros();
    if mips > most {
        return Err(PlayerTextureError::Mips {
            mips,
            width: stored.width,
            height: stored.height,
        });
    }
    Ok(())
}

/// The `GraphicsFormat` of a texture of layers or a volume read as RGBA8 texels, or the refusal of
/// a format that is not. `noun` says what the texture is.
pub(super) fn texel_format(stored: &PlayerTexture, noun: &str) -> Result<u32, PlayerTextureError> {
    if !ARRAY_FORMATS.contains(&stored.format) {
        return Err(PlayerTextureError::Unsupported {
            format: format!("{} {noun}", crate::textures::stored_format_name(stored)),
        });
    }
    Ok(if stored.linear {
        RGBA8_LINEAR
    } else {
        RGBA8_SRGB
    })
}

/// The bytes of one texel of a stored texture that holds texels rather than blocks: one for
/// `TextureFormat` 1, three for 3, and four for the others [`texel_format`] admits and for the
/// four-byte `GraphicsFormat`s 87 and 88. A caller refuses every other format before it counts.
pub(super) fn texel_bytes(stored: &PlayerTexture) -> usize {
    match (stored.graphics_format, stored.format) {
        (FOUR_BYTE_ARRAY | FOUR_BYTE_DATA, _) => 4,
        (_, 1) => 1,
        (_, 3) => 3,
        _ => 4,
    }
}

/// The layers of `stored` as the device takes them: the stored bytes, when each layer is the whole
/// chain of its levels in `layout`, which for blocks needs a device that samples them and a base
/// level of whole blocks.
fn verbatim_layers(
    stored: &PlayerTexture,
    layout: &Verbatim,
    blocks: bool,
) -> Result<Vec<u8>, PlayerTextureError> {
    if layout.side == BLOCK_SIDE {
        if !blocks {
            return Err(PlayerTextureError::ArrayNeedsBlocks {
                format: layout.name,
            });
        }
        if !stored.width.is_multiple_of(BLOCK_SIDE) || !stored.height.is_multiple_of(BLOCK_SIDE) {
            return Err(PlayerTextureError::ArrayBlockSize {
                format: layout.name,
                width: stored.width,
                height: stored.height,
            });
        }
    }
    let mips = stored.mips.max(1);
    let needed = layout.layer_bytes(stored, mips);
    let actual = stored.bytes.len() / stored.depth as usize;
    if actual != needed {
        return Err(PlayerTextureError::Levels {
            mips,
            needed,
            actual,
        });
    }
    if layout.blue_first {
        return Ok(red_first(&stored.bytes));
    }
    Ok(stored.bytes.clone())
}

/// Four-byte texels stored blue, green, red, alpha ([`FOUR_BYTE_ARRAY`]) as red, green, blue,
/// alpha, the order the device's RGBA8 has. Every array, cube and volume that uploads 87 goes
/// through it.
pub(super) fn red_first(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks_exact(4)
        .flat_map(|texel| [texel[2], texel[1], texel[0], texel[3]])
        .collect()
}

/// Every layer of `stored` as RGBA8 texels, each layer's levels one after the other.
fn rgba8_layers(stored: &PlayerTexture, mips: u32) -> Result<Vec<u8>, PlayerTextureError> {
    let length = stored.bytes.len() / stored.depth as usize;
    let mut bytes = Vec::new();
    for layer in stored.bytes.chunks_exact(length) {
        bytes.extend(layer_rgba8(stored, layer, mips)?);
    }
    Ok(bytes)
}

/// One layer's `mips` levels as RGBA8, base level first. A level is as wide and as high as the
/// base level shifted down, and at least one texel. A layer too short for its base level is the
/// plain texture's short error; any other length that is not the whole chain is a level error.
fn layer_rgba8(
    stored: &PlayerTexture,
    layer: &[u8],
    mips: u32,
) -> Result<Vec<u8>, PlayerTextureError> {
    let per_texel = texel_bytes(stored);
    let side = |side: u32, level: u32| side.checked_shr(level).unwrap_or(0).max(1);
    let sizes: Vec<[u32; 2]> = (0..mips)
        .map(|level| [side(stored.width, level), side(stored.height, level)])
        .collect();
    let lengths: Vec<usize> = sizes
        .iter()
        .map(|[width, height]| *width as usize * *height as usize * per_texel)
        .collect();
    let needed = lengths.iter().sum();
    if layer.len() < lengths[0] {
        return level_rgba8(stored, sizes[0], layer);
    }
    if layer.len() != needed {
        return Err(PlayerTextureError::Levels {
            mips,
            needed,
            actual: layer.len(),
        });
    }
    let mut rgba = Vec::new();
    let mut rest = layer;
    for (size, length) in sizes.into_iter().zip(lengths) {
        let (level, tail) = rest.split_at(length);
        rgba.extend(level_rgba8(stored, size, level)?);
        rest = tail;
    }
    Ok(rgba)
}

/// One level of `stored`, of `size` texels, as RGBA8.
fn level_rgba8(
    stored: &PlayerTexture,
    [width, height]: [u32; 2],
    bytes: &[u8],
) -> Result<Vec<u8>, PlayerTextureError> {
    player_rgba8(&PlayerTexture {
        width,
        height,
        mips: 1,
        depth: 0,
        dimension: TextureDimension::D2,
        bytes: bytes.to_vec(),
        ..*stored
    })
}
