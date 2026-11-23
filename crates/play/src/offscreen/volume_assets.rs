//! A stored 3D texture as render-program's source volume.
//!
//! The managed engine stores a volume's bytes as the mip chain it keeps: the levels one after the
//! other from the base level, each holding its slices one after the other, and a level is as wide,
//! as high and as deep as the base level shifted down, and at least one texel. Rows stay bottom
//! first, as for every stored texture. A device samples block-compressed volumes only with a
//! feature the host does not enable, so a volume goes up as texels: Alpha8, RGB24 and RGBA32 read
//! as RGBA8 as a plain texture is, and the four-byte `GraphicsFormats` 87 and 88 as the texels they
//! hold, 87 with its blue and red bytes exchanged.

use worldrunner_disunity_runtime::{PlayerTexture, TextureDimension};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::SamplerState;
use worldrunner_render_program::unity::offscreen::frame::{SourceDimension, SourceTexture};

use super::assets::RGBA8_SRGB;
use super::layer_assets::{
    FOUR_BYTE_ARRAY, FOUR_BYTE_DATA, four_byte_data, red_first, texel_bytes, texel_format,
};
use crate::textures::{PlayerTextureError, player_rgba8};

/// A stored volume as a source volume of `depth` slices.
///
/// # Errors
/// Fails when the volume has no slices or bytes, states more levels than its size has, is in a
/// format that is not read as texels, or its bytes are not exactly the chain of its levels.
pub(super) fn volume_of(
    id: TextureId,
    stored: &PlayerTexture,
    sampler: Option<SamplerState>,
) -> Result<SourceTexture, PlayerTextureError> {
    let size = [stored.width, stored.height, stored.depth];
    if stored.bytes.is_empty() || size.contains(&0) {
        return Err(PlayerTextureError::Layers {
            depth: stored.depth,
            actual: stored.bytes.len(),
        });
    }
    let mips = stored.mips.max(1);
    let most = u32::BITS - size.into_iter().max().unwrap_or(1).leading_zeros();
    if mips > most {
        return Err(PlayerTextureError::VolumeMips { mips, size });
    }
    let (graphics_format, bytes) = match stored.graphics_format {
        FOUR_BYTE_ARRAY => (RGBA8_SRGB, red_first(&verbatim(stored, size, mips)?)),
        FOUR_BYTE_DATA => (four_byte_data(stored), verbatim(stored, size, mips)?),
        _ => (
            texel_format(stored, "3D")?,
            rgba8_levels(stored, size, mips)?,
        ),
    };
    Ok(SourceTexture {
        id,
        dimension: SourceDimension::D3(stored.depth),
        size: [stored.width, stored.height],
        graphics_format,
        mips,
        bytes,
        sampler,
    })
}

/// The width, height and depth of each of `mips` levels.
fn level_sizes(size: [u32; 3], mips: u32) -> Vec<[u32; 3]> {
    (0..mips)
        .map(|level| size.map(|side| side.checked_shr(level).unwrap_or(0).max(1)))
        .collect()
}

/// The bytes of the texels of one level, `per_texel` bytes each.
fn level_len([width, height, depth]: [u32; 3], per_texel: usize) -> usize {
    width as usize * height as usize * depth as usize * per_texel
}

/// Checks that the bytes are exactly the chain of levels, and returns the length of each.
fn level_lengths(
    stored: &PlayerTexture,
    size: [u32; 3],
    mips: u32,
) -> Result<Vec<usize>, PlayerTextureError> {
    let texel = texel_bytes(stored);
    let lengths: Vec<usize> = level_sizes(size, mips)
        .into_iter()
        .map(|level| level_len(level, texel))
        .collect();
    let needed: usize = lengths.iter().sum();
    if stored.bytes.len() != needed {
        return Err(PlayerTextureError::Levels {
            mips,
            needed,
            actual: stored.bytes.len(),
        });
    }
    Ok(lengths)
}

/// The stored bytes, when they are exactly the chain of levels of four-byte texels.
fn verbatim(
    stored: &PlayerTexture,
    size: [u32; 3],
    mips: u32,
) -> Result<Vec<u8>, PlayerTextureError> {
    level_lengths(stored, size, mips)?;
    Ok(stored.bytes.clone())
}

/// Every level as RGBA8 texels, one after the other. A level is read as a 2D texture as wide as
/// the level and as high as all its slices, which keeps the slices in the order they are stored.
fn rgba8_levels(
    stored: &PlayerTexture,
    size: [u32; 3],
    mips: u32,
) -> Result<Vec<u8>, PlayerTextureError> {
    let lengths = level_lengths(stored, size, mips)?;
    let mut rgba = Vec::new();
    let mut rest = stored.bytes.as_slice();
    for ([width, height, depth], length) in level_sizes(size, mips).into_iter().zip(lengths) {
        let (level, tail) = rest.split_at(length);
        rgba.extend(player_rgba8(&PlayerTexture {
            width,
            height: height * depth,
            mips: 1,
            depth: 0,
            dimension: TextureDimension::D2,
            bytes: level.to_vec(),
            ..*stored
        })?);
        rest = tail;
    }
    Ok(rgba)
}
