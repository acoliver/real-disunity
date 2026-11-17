//! Conversion of validated texture sources into payload-free descriptors.
//!
//! Validation reuses [`prepare_texture`](super::prepare_texture) and the container reader
//! behind it, which borrow the source bytes. No payload is copied or decoded.
use super::{TextureError, parse_dds, prepare_texture};
use thiserror::Error;
use worldrunner_render_plan::{
    SceneDdsDescriptor, SceneResources, SceneTexture, SceneTextureDescriptor, SceneValidationError,
    TextureColorSpace, TextureEncoding,
};

/// A texture table could not be converted to descriptors.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TextureReleaseError {
    /// One texture failed the validation that preparation applies.
    #[error("texture {index} cannot be released: {source}")]
    Texture {
        /// Position of the first failing texture in the table.
        index: usize,
        /// The validation failure.
        #[source]
        source: TextureError,
    },
    /// The descriptor table did not satisfy the scene's resource invariants.
    #[error("released texture table is invalid: {0}")]
    Table(#[from] SceneValidationError),
}

/// Describes one texture without its payload.
///
/// Validates the upload layout through [`prepare_texture`] or its borrowed container
/// parser, including every mip level. DDS headers must also describe a single-layer,
/// two-dimensional, non-cube texture. Every returned descriptor satisfies its domain
/// invariants, including an already-released descriptor. No source bytes are copied.
///
/// # Errors
///
/// Returns the [`TextureError`] that preparing the texture would return. Encodings that
/// cannot be uploaded, such as KTX containers, report [`TextureError::UnsupportedEncoding`].
/// Containers that are not a single two-dimensional, non-cube texture report
/// [`TextureError::UnsupportedDdsLayout`]. The returned descriptor satisfies
/// [`SceneTextureDescriptor::validate`]; a descriptor that does not, including a forged
/// released texture, reports [`TextureError::Descriptor`].
pub fn describe_texture(texture: &SceneTexture) -> Result<SceneTextureDescriptor, TextureError> {
    let descriptor = describe_payload(texture)?;
    descriptor.validate()?;
    Ok(descriptor)
}

fn describe_payload(texture: &SceneTexture) -> Result<SceneTextureDescriptor, TextureError> {
    match texture.payload() {
        SceneTexture::Released(descriptor) => Ok(*descriptor),
        SceneTexture::Encoded {
            encoding,
            color_space,
            bytes,
        } => describe_encoded(*encoding, *color_space, bytes),
        SceneTexture::Sampled { .. } => unreachable!("payload removes sampling wrapper"),
        described @ (SceneTexture::Texels { .. } | SceneTexture::SpecularCube(_)) => {
            prepare_texture(described)?;
            Ok(described
                .domain_descriptor()
                .expect("texel grids and cubes describe themselves"))
        }
    }
}

fn describe_encoded(
    encoding: TextureEncoding,
    declared: TextureColorSpace,
    bytes: &[u8],
) -> Result<SceneTextureDescriptor, TextureError> {
    if encoding != TextureEncoding::DdsContainer {
        return Err(TextureError::UnsupportedEncoding { encoding });
    }
    let parsed = parse_dds(bytes, declared)?;
    // `parse_dds` ran first so malformed headers keep their precise errors. The shared
    // header reader then rejects arrays, cubes and non-2D dimensions from the borrowed
    // bytes; it does not look at the payload, so trailing bytes stay admitted.
    worldrunner_texture::TextureFormat::from_dx10_dds(bytes)
        .map_err(|_| TextureError::UnsupportedDdsLayout)?;
    Ok(SceneTextureDescriptor::Dds(SceneDdsDescriptor {
        format: parsed.format,
        color_space: declared,
        width: parsed.width,
        height: parsed.height,
        mips: u32::try_from(parsed.mips.len()).expect("validated mip chain has at most 32 levels"),
        logical_bytes: bytes.len() as u64,
    }))
}

/// Replaces the table's textures with validated descriptors.
///
/// Materials and dense handles are unchanged. Every source is validated before the
/// result is built, so the first failure leaves the caller with the original table, which is
/// never modified. Converting a table that is already released returns an equal table.
/// The input and its clones keep their payloads: this call frees no memory.
///
/// # Errors
///
/// Returns [`TextureReleaseError::Texture`] naming the first texture that cannot be
/// described, or [`TextureReleaseError::Table`] if the descriptor table fails validation.
pub fn release_texture_sources(
    resources: &SceneResources,
) -> Result<SceneResources, TextureReleaseError> {
    let released = resources
        .textures()
        .iter()
        .enumerate()
        .map(|(index, texture)| {
            release_texture(texture)
                .map_err(|source| TextureReleaseError::Texture { index, source })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(resources.with_textures(released)?)
}

/// Replaces a texture's payload with its descriptor, keeping any sampling wrapper.
fn release_texture(texture: &SceneTexture) -> Result<SceneTexture, TextureError> {
    match texture {
        SceneTexture::Sampled { texture, sampler } => Ok(SceneTexture::Sampled {
            texture: Box::new(release_texture(texture)?),
            sampler: *sampler,
        }),
        other => describe_texture(other).map(SceneTexture::Released),
    }
}
