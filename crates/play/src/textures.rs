//! A plan's textures as scene textures, under the identities the managed loader gives them.
use worldrunner_disunity_runtime::PlayerTexture;
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
        "a {format} texture of height {height} cannot be turned over, as its height is not a multiple of 4"
    )]
    Unaligned { format: &'static str, height: u32 },
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
        10 => "DXT1".into(),
        12 => "DXT5".into(),
        24 => "BC6H".into(),
        25 => "BC7".into(),
        26 => "BC4".into(),
        27 => "BC5".into(),
        28 => "DXT1Crunched".into(),
        29 => "DXT5Crunched".into(),
        other => format!("TextureFormat {other}"),
    }
}

/// The base level of a texture the managed engine stored, as a scene texture with its rows top
/// first. RGBA32, RGB24 and Alpha8 become RGBA8 texels (Alpha8 as white with that alpha, or with the coverage of its distance field when the texture holds one). DXT1,
/// DXT5 and BC4 follow `path`: one-level DX10 DDS containers in their sRGB forms when the texture
/// is not linear (BC4 holds data and is always linear), or decoded RGBA8 texels (BC4 as red). BC7 is only drawn decoded.
/// Further mip levels are not carried, as the engine's choice for now.
pub fn player_scene_texture(
    texture: &PlayerTexture,
    path: BlockPath,
) -> Result<SceneTexture, PlayerTextureError> {
    match texture.format {
        4 => texels(texture, "RGBA32", 4, |t| [t[0], t[1], t[2], t[3]]),
        3 => texels(texture, "RGB24", 3, |t| [t[0], t[1], t[2], 255]),
        1 => texels(texture, "Alpha8", 1, |t| {
            [
                255,
                255,
                255,
                alpha8_coverage(t[0], texture.distance_spread),
            ]
        }),
        10 => block(texture, "DXT1", BlockLayout::Bc1, path),
        12 => block(texture, "DXT5", BlockLayout::Bc3, path),
        25 => block(texture, "BC7", BlockLayout::Bc7, path),
        26 => block(texture, "BC4", BlockLayout::Bc4, path),
        other => Err(PlayerTextureError::Unsupported {
            format: format_name(other),
        }),
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
    let slope = 2. * spread as f32;
    let distance = f32::from(texel) / 255.;
    let coverage = ((distance - 0.5) * slope + 0.5).clamp(0., 1.);
    (coverage * 255.).round() as u8
}

fn color_space(linear: bool) -> TextureColorSpace {
    if linear {
        TextureColorSpace::Linear
    } else {
        TextureColorSpace::Srgb
    }
}

fn texels(
    texture: &PlayerTexture,
    format: &'static str,
    per_texel: usize,
    convert: impl Fn(&[u8]) -> [u8; 4],
) -> Result<SceneTexture, PlayerTextureError> {
    let (width, height) = (texture.width, texture.height);
    let needed = width as usize * per_texel * height as usize;
    let base = texture
        .bytes
        .get(..needed)
        .ok_or(PlayerTextureError::Short {
            format,
            width,
            height,
            needed,
            actual: texture.bytes.len(),
        })?;
    let rgba: Vec<u8> = base.chunks_exact(per_texel).flat_map(&convert).collect();
    Ok(rgba_texels(texture, &rgba))
}

/// RGBA8 rows stored bottom first, as a scene texture with its rows top first.
fn rgba_texels(texture: &PlayerTexture, rgba: &[u8]) -> SceneTexture {
    let row = texture.width as usize * 4;
    let top_first: Vec<u8> = rgba.chunks_exact(row).rev().flatten().copied().collect();
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: color_space(texture.linear),
        width: texture.width,
        height: texture.height,
        mips: 1,
        bytes: top_first.into_boxed_slice(),
    }
}

fn block(
    texture: &PlayerTexture,
    format: &'static str,
    layout: BlockLayout,
    path: BlockPath,
) -> Result<SceneTexture, PlayerTextureError> {
    let (width, height) = (texture.width, texture.height);
    if path == BlockPath::Dds && layout == BlockLayout::Bc7 {
        return Err(PlayerTextureError::Unsupported {
            format: format.into(),
        });
    }
    if path == BlockPath::Dds && height % 4 != 0 {
        return Err(PlayerTextureError::Unaligned { format, height });
    }
    let needed = level_bytes(layout, width, height);
    let base = texture
        .bytes
        .get(..needed)
        .ok_or(PlayerTextureError::Short {
            format,
            width,
            height,
            needed,
            actual: texture.bytes.len(),
        })?;
    if path == BlockPath::Rgba8 {
        return Ok(rgba_texels(
            texture,
            &decode_level(layout, width, height, base),
        ));
    }
    let srgb = !texture.linear && layout != BlockLayout::Bc4;
    Ok(SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: color_space(!srgb),
        bytes: dx10_dds(
            width,
            height,
            layout.dxgi(srgb),
            &flip_rows(layout, width, base),
        )
        .into_boxed_slice(),
    })
}
