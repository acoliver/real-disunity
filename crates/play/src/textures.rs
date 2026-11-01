//! A plan's textures as scene textures, under the identities the managed loader gives them.
use worldrunner_disunity_runtime::PlayerTexture;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding, TextureFilter, TextureSampler,
    TextureWrap, simulation::SpriteTextures,
};
use worldrunner_unity_import::{Filter, PlanTexture, Project, Wrap, import_texture};

use crate::{
    PlayError,
    block_texture::{BlockLayout, dx10_dds, flip_rows, level_bytes},
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
/// first. RGBA32, RGB24 and Alpha8 become RGBA8 texels (Alpha8 as white with that alpha); DXT1,
/// DXT5 and BC4 become one-level DX10 DDS containers in their sRGB forms when the texture is not
/// linear (BC4 holds data and is always linear). Further mip levels are not carried, as the
/// engine's choice for now.
pub fn player_scene_texture(texture: &PlayerTexture) -> Result<SceneTexture, PlayerTextureError> {
    match texture.format {
        4 => texels(texture, "RGBA32", 4, |t| [t[0], t[1], t[2], t[3]]),
        3 => texels(texture, "RGB24", 3, |t| [t[0], t[1], t[2], 255]),
        1 => texels(texture, "Alpha8", 1, |t| [255, 255, 255, t[0]]),
        10 => block(texture, "DXT1", BlockLayout::Bc1),
        12 => block(texture, "DXT5", BlockLayout::Bc3),
        26 => block(texture, "BC4", BlockLayout::Bc4),
        other => Err(PlayerTextureError::Unsupported {
            format: format_name(other),
        }),
    }
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
    let row = width as usize * per_texel;
    let needed = row * height as usize;
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
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for line in base.chunks_exact(row).rev() {
        rgba.extend(line.chunks_exact(per_texel).flat_map(&convert));
    }
    Ok(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: color_space(texture.linear),
        width,
        height,
        mips: 1,
        bytes: rgba.into_boxed_slice(),
    })
}

fn block(
    texture: &PlayerTexture,
    format: &'static str,
    layout: BlockLayout,
) -> Result<SceneTexture, PlayerTextureError> {
    let (width, height) = (texture.width, texture.height);
    if height % 4 != 0 {
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
