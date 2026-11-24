//! A plan's textures as scene textures, under the identities the managed loader gives them.
use worldrunner_disunity_runtime::{PlayerTexture, TextureDimension};
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding, TextureFilter, TextureSampler,
    TextureWrap, simulation::SpriteTextures,
};
use worldrunner_unity_import::{Filter, PlanTexture, Project, Wrap, import_texture};

use crate::{
    PlayError,
    block_texture::{BlockLayout, decode_level, dx10_dds, flip_rows, level_bytes},
};

/// Decodes texture `i` of the plan into identity `first + i`, with its import sampling.
pub fn scene_textures(
    project: &Project,
    textures: &[PlanTexture],
    first: u64,
) -> Result<SpriteTextures, PlayError> {
    let mut result = SpriteTextures::default();
    for (identity, planned) in (first..).zip(textures) {
        let guid = planned
            .guid
            .parse()
            .map_err(|_| PlayError::Invalid(format!("{}: invalid GUID", planned.path)))?;
        let image = import_texture(project, guid)?.image().clone();
        let texels = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: if planned.srgb {
                TextureColorSpace::Srgb
            } else {
                TextureColorSpace::Linear
            },
            width: image.width,
            height: image.height,
            mips: 1,
            bytes: image.rgba8.into_boxed_slice(),
        };
        let sampler = TextureSampler {
            filter: match planned.filter {
                Filter::Point => TextureFilter::Point,
                Filter::Bilinear => TextureFilter::Bilinear,
                Filter::Trilinear => TextureFilter::Trilinear,
            },
            wrap_u: wrap(planned, planned.wrap[0])?,
            wrap_v: wrap(planned, planned.wrap[1])?,
        };
        result
            .textures
            .insert(identity, texels.with_sampler(sampler));
    }
    Ok(result)
}

fn wrap(texture: &PlanTexture, wrap: Wrap) -> Result<TextureWrap, PlayError> {
    match wrap {
        Wrap::Repeat => Ok(TextureWrap::Repeat),
        Wrap::Clamp => Ok(TextureWrap::Clamp),
        Wrap::Mirror => Ok(TextureWrap::Mirror),
        Wrap::MirrorOnce => Err(PlayError::Invalid(format!(
            "{}: mirror-once wrapping is not supported",
            texture.path
        ))),
    }
}

/// Why a player's texture has no scene texture.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlayerTextureError {
    #[error("{format} textures are not drawn yet")]
    Unsupported { format: String },
    #[error("a {dimension:?} texture has no single image to draw as a flat texture")]
    NotFlat { dimension: TextureDimension },
    #[error(
        "a {width}x{height} {format} texture holds {actual} bytes but its base level needs {needed}"
    )]
    Short {
        format: &'static str,
        width: u32,
        height: u32,
        needed: usize,
        actual: usize,
    },
    #[error(
        "a {format} level of height {height} cannot be turned over, as its height is neither less than 4 nor a multiple of 4"
    )]
    Unaligned { format: &'static str, height: u32 },
    #[error("a cube has six square faces, and this one has {depth} layers of {width}x{height}")]
    CubeFaces { depth: u32, width: u32, height: u32 },
    #[error("{mips} mip levels are more than a {}x{}x{} volume has", .size[0], .size[1], .size[2])]
    VolumeMips { mips: u32, size: [u32; 3] },
    #[error("a texture of {depth} layers holds {actual} bytes, which are not {depth} equal layers")]
    Layers { depth: u32, actual: usize },
    #[error("{mips} mip levels are more than a {width}x{height} texture has")]
    Mips { mips: u32, width: u32, height: u32 },
    #[error(
        "a {format} array or cube is drawn only as blocks, and this device has no block compression"
    )]
    ArrayNeedsBlocks { format: &'static str },
    #[error("a {width}x{height} {format} array or cube is not made of whole 4x4 blocks")]
    ArrayBlockSize {
        format: &'static str,
        width: u32,
        height: u32,
    },
    #[error("a layer of {mips} mip levels holds {actual} bytes but the levels take {needed}")]
    Levels {
        mips: u32,
        needed: usize,
        actual: usize,
    },
}

/// How a block-compressed texture reaches the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockPath {
    /// A DX10 DDS container; the renderer's device must have been created with block compression.
    Dds,
    /// RGBA8 texels decoded here, which every device can sample.
    Rgba8,
}

/// The name of an engine `TextureFormat` value, for logs.
#[must_use]
pub fn format_name(format: u32) -> String {
    match format {
        1 => "Alpha8".into(),
        3 => "RGB24".into(),
        4 => "RGBA32".into(),
        5 => "ARGB32".into(),
        10 => "DXT1".into(),
        12 => "DXT5".into(),
        24 => "BC6H".into(),
        25 => "BC7".into(),
        14 => "BGRA32".into(),
        26 => "BC4".into(),
        27 => "BC5".into(),
        28 => "DXT1Crunched".into(),
        29 => "DXT5Crunched".into(),
        other => format!("TextureFormat {other}"),
    }
}

/// The name of a stored texture's format, for logs: its engine `TextureFormat`, or for a format the
/// engine has none for, as a 2D array may be, its `GraphicsFormat`.
#[must_use]
pub fn stored_format_name(texture: &PlayerTexture) -> String {
    if texture.format == 0 && texture.graphics_format != 0 {
        format!("GraphicsFormat {}", texture.graphics_format)
    } else {
        format_name(texture.format)
    }
}

/// A texture the managed engine stored, as a scene texture with its mip levels and its rows top
/// first. RGBA32, RGB24 and Alpha8 become RGBA8 texels (Alpha8 as white with that alpha, or with the coverage of its distance field when the texture holds one). DXT1,
/// DXT5 and BC4 follow `path`: DX10 DDS containers in their sRGB forms when the texture is not
/// linear (BC4 holds data and is always linear), or decoded RGBA8 texels (BC4 as red). BC7 is only
/// drawn decoded. Every level the texture holds is carried, each turned over alone; a texture
/// whose `mips` count is more than its size has levels for is refused with
/// [`PlayerTextureError::Mips`], and one whose bytes stop short of its levels with
/// [`PlayerTextureError::Levels`]. A texture of layers (an array, a cube or a volume) is refused: a
/// sprite, a UI graphic or a mesh slot draws one image.
pub fn player_scene_texture(
    texture: &PlayerTexture,
    path: BlockPath,
) -> Result<SceneTexture, PlayerTextureError> {
    if texture.dimension != TextureDimension::D2 {
        return Err(PlayerTextureError::NotFlat {
            dimension: texture.dimension,
        });
    }
    if path == BlockPath::Dds
        && let Some((format, layout)) = block_format(texture.format)
    {
        return block(texture, format, layout);
    }
    rgba_levels(texture)
}

/// The name and layout of a block-compressed engine `TextureFormat` this host decodes.
fn block_format(format: u32) -> Option<(&'static str, BlockLayout)> {
    match format {
        10 => Some(("DXT1", BlockLayout::Bc1)),
        12 => Some(("DXT5", BlockLayout::Bc3)),
        25 => Some(("BC7", BlockLayout::Bc7)),
        26 => Some(("BC4", BlockLayout::Bc4)),
        _ => None,
    }
}

/// One mip level of a stored texture: its size and the bytes from its first byte to the end of
/// the texture's bytes.
#[derive(Clone, Copy)]
struct Level<'a> {
    width: u32,
    height: u32,
    bytes: &'a [u8],
}

/// The bytes of a `width` by `height` level of the engine `TextureFormat` `format`.
fn level_size(format: u32, width: u32, height: u32) -> Result<usize, PlayerTextureError> {
    let texels = width as usize * height as usize;
    match format {
        4 | 5 | 14 => Ok(texels * 4),
        3 => Ok(texels * 3),
        1 => Ok(texels),
        other => block_format(other)
            .map(|(_, layout)| level_bytes(layout, width, height))
            .ok_or(PlayerTextureError::Unsupported {
                format: format_name(other),
            }),
    }
}

/// The mip chain of `texture`, the base level first, each level halving the size down to one
/// texel.
fn levels(texture: &PlayerTexture) -> Result<Vec<Level<'_>>, PlayerTextureError> {
    let count = texture.mips.max(1);
    let (width, height) = (texture.width, texture.height);
    if count > width.max(height).max(1).ilog2() + 1 {
        return Err(PlayerTextureError::Mips {
            mips: count,
            width,
            height,
        });
    }
    let mut chain = Vec::new();
    let mut offset = 0;
    for level in 0..count {
        let (width, height) = ((width >> level).max(1), (height >> level).max(1));
        chain.push(Level {
            width,
            height,
            bytes: texture.bytes.get(offset..).unwrap_or_default(),
        });
        offset += level_size(texture.format, width, height)?;
    }
    let base = level_size(texture.format, texture.width, texture.height)?;
    if count > 1 && texture.bytes.len() >= base && texture.bytes.len() < offset {
        return Err(PlayerTextureError::Levels {
            mips: count,
            needed: offset,
            actual: texture.bytes.len(),
        });
    }
    Ok(chain)
}

/// The base level of a texture the managed engine stored as RGBA8 texels, rows bottom first as
/// the engine stores them. Formats are read as [`player_scene_texture`] reads them: RGBA32, RGB24
/// and Alpha8 directly, DXT1, DXT5, BC7 and BC4 decoded.
pub fn player_rgba8(texture: &PlayerTexture) -> Result<Vec<u8>, PlayerTextureError> {
    let base = Level {
        width: texture.width,
        height: texture.height,
        bytes: &texture.bytes,
    };
    level_rgba8(texture.format, texture.distance_spread, base)
}

/// The RGBA8 texels of one level of a texture of engine `TextureFormat` `format`, rows bottom first.
fn level_rgba8(format: u32, spread: u32, level: Level<'_>) -> Result<Vec<u8>, PlayerTextureError> {
    match format {
        4 => texels(level, "RGBA32", 4, |t| [t[0], t[1], t[2], t[3]]),
        3 => texels(level, "RGB24", 3, |t| [t[0], t[1], t[2], 255]),
        5 => texels(level, "ARGB32", 4, |t| [t[1], t[2], t[3], t[0]]),
        14 => texels(level, "BGRA32", 4, |t| [t[2], t[1], t[0], t[3]]),
        1 => texels(level, "Alpha8", 1, |t| {
            [255, 255, 255, alpha8_coverage(t[0], spread)]
        }),
        other => match block_format(other) {
            Some((name, layout)) => block_rgba8(level, name, layout),
            None => Err(PlayerTextureError::Unsupported {
                format: format_name(other),
            }),
        },
    }
}

/// The coverage of an `Alpha8` texel. Coverage atlases (`spread` zero) are used as they are. A
/// distance field byte `d` (0.5 at a glyph's edge, `1 / (2 * spread)` per texel of distance)
/// becomes `clamp((d - 0.5) * 2 * spread + 0.5, 0, 1)`: 0 outside, 1 inside, and a ramp one texel
/// wide across the edge, the smoothing width the engine chooses for text drawn without a
/// distance field material.
fn alpha8_coverage(texel: u8, spread: u32) -> u8 {
    if spread == 0 {
        return texel;
    }
    // In bytes, the coverage is (2 * texel - 255) * spread + 127.5, rounded half up.
    let scaled = (2 * i64::from(texel) - 255) * i64::from(spread) + 128;
    u8::try_from(scaled.clamp(0, 255)).unwrap_or(u8::MAX)
}

fn color_space(linear: bool) -> TextureColorSpace {
    if linear {
        TextureColorSpace::Linear
    } else {
        TextureColorSpace::Srgb
    }
}

fn texels(
    level: Level<'_>,
    format: &'static str,
    per_texel: usize,
    convert: impl Fn(&[u8]) -> [u8; 4],
) -> Result<Vec<u8>, PlayerTextureError> {
    let needed = level.width as usize * per_texel * level.height as usize;
    let bytes = level.bytes.get(..needed).ok_or(PlayerTextureError::Short {
        format,
        width: level.width,
        height: level.height,
        needed,
        actual: level.bytes.len(),
    })?;
    Ok(bytes.chunks_exact(per_texel).flat_map(&convert).collect())
}

/// The levels of `texture` as RGBA8 texels, each with its rows turned top first, one after the
/// other.
fn rgba_levels(texture: &PlayerTexture) -> Result<SceneTexture, PlayerTextureError> {
    let chain = levels(texture)?;
    let mut bytes = Vec::new();
    for level in &chain {
        let rgba = level_rgba8(texture.format, texture.distance_spread, *level)?;
        let row = level.width as usize * 4;
        bytes.extend(rgba.chunks_exact(row).rev().flatten());
    }
    Ok(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: color_space(texture.linear),
        width: texture.width,
        height: texture.height,
        mips: u32::try_from(chain.len()).unwrap_or(u32::MAX),
        bytes: bytes.into_boxed_slice(),
    })
}

/// A block-compressed level decoded to RGBA8, rows bottom first.
fn block_rgba8(
    level: Level<'_>,
    format: &'static str,
    layout: BlockLayout,
) -> Result<Vec<u8>, PlayerTextureError> {
    let (width, height) = (level.width, level.height);
    let needed = level_bytes(layout, width, height);
    let bytes = level.bytes.get(..needed).ok_or(PlayerTextureError::Short {
        format,
        width,
        height,
        needed,
        actual: level.bytes.len(),
    })?;
    Ok(decode_level(layout, width, height, bytes))
}

/// A block-compressed texture as a DX10 DDS container of all its levels, each turned over alone,
/// in the texture's own color space. A level can be turned over when its height is a multiple of
/// four or less than four.
fn block(
    texture: &PlayerTexture,
    format: &'static str,
    layout: BlockLayout,
) -> Result<SceneTexture, PlayerTextureError> {
    if layout == BlockLayout::Bc7 {
        return Err(PlayerTextureError::Unsupported {
            format: format.into(),
        });
    }
    let chain = levels(texture)?;
    let mut payload = Vec::new();
    for level in &chain {
        let (width, height) = (level.width, level.height);
        if height % 4 != 0 && height > 4 {
            return Err(PlayerTextureError::Unaligned { format, height });
        }
        let needed = level_bytes(layout, width, height);
        let bytes = level.bytes.get(..needed).ok_or(PlayerTextureError::Short {
            format,
            width,
            height,
            needed,
            actual: level.bytes.len(),
        })?;
        payload.extend(flip_rows(layout, width, height, bytes));
    }
    let base = level_bytes(layout, texture.width, texture.height);
    let srgb = !texture.linear && layout != BlockLayout::Bc4;
    Ok(SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: color_space(!srgb),
        bytes: dx10_dds(
            (texture.width, texture.height),
            layout.dxgi(srgb),
            u32::try_from(chain.len()).unwrap_or(u32::MAX),
            (base, &payload),
        )
        .into_boxed_slice(),
    })
}
