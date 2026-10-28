use super::{SceneTexture, SceneValidationError, TexelFormat, TextureColorSpace, TextureHandle};

/// Screen-space coverage for object and impostor distance fades.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneFadeCoverage {
    /// Stable per-pixel hashing, independent of caller textures.
    Hashed,
    /// Point/repeat sampling at `(fragment_position - jitter) / texture_size`.
    /// The texture must be a square, linear R8 texel grid with one mip.
    Threshold {
        texture: TextureHandle,
        jitter_pixels: [i32; 2],
    },
}

/// Precision of the distance fade factor, independent of alpha cutouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneFadeQuantization {
    Continuous,
    /// Truncate the factor to one of 256 values before coverage testing.
    Byte,
}

impl SceneFadeCoverage {
    pub(super) fn validate(self, textures: &[SceneTexture]) -> Result<(), SceneValidationError> {
        let Self::Threshold { texture, .. } = self else {
            return Ok(());
        };
        let Some(value) = textures.get(texture.raw() as usize) else {
            return Err(SceneValidationError::HandleOutOfRange {
                part: "fade threshold",
                resource: "texture",
                handle: texture.raw(),
                limit: textures.len() as u64,
            });
        };
        if threshold_texels(value).is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "fade threshold requires square linear single-mip R8 texels",
            });
        }
        Ok(())
    }
}

/// Returns the validated threshold grid without decoding or resampling its values.
pub fn threshold_texels(texture: &SceneTexture) -> Option<(u32, &[u8])> {
    match texture {
        SceneTexture::Sampled { texture, .. } => threshold_texels(texture),
        SceneTexture::Texels {
            format: TexelFormat::R8Unorm,
            color_space: TextureColorSpace::Linear,
            width,
            height,
            mips: 1,
            bytes,
        } if width == height => Some((*width, bytes)),
        _ => None,
    }
}
