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

/// Which side of the shared pixel threshold a distance fade accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneFadeDirection {
    /// Existing inclusive comparison: `coverage >= threshold`.
    #[default]
    Inclusive,
    /// Lower partition: `threshold < coverage`. Pair with `Complement`.
    Keep,
    /// Upper partition: `threshold >= 1 - coverage`, including equality.
    /// Quantization applies to the common lower-partition boundary so paired
    /// bands remain complementary even when their continuous factors round.
    Complement,
}

/// Object-local distance reference, transformed by the authored instance matrix.
/// This affects distance fading only, not placement, wind or LOD selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SceneFadeReference {
    offset_millimeters: [i32; 3],
    eye_vertical_clamp_millimeters: Option<u32>,
}

impl SceneFadeReference {
    pub const fn new(offset_millimeters: [i32; 3]) -> Self {
        Self {
            offset_millimeters,
            eye_vertical_clamp_millimeters: None,
        }
    }

    /// Clamp the eye's Z to at least reference Z plus this nonnegative height.
    pub fn with_eye_vertical_clamp(
        mut self,
        millimeters: u32,
    ) -> Result<Self, SceneValidationError> {
        if millimeters > 1_000_000 {
            return Err(SceneValidationError::RangeViolation {
                part: "fade eye vertical clamp millimeters",
                minimum: 0,
                maximum: 1_000_000,
                observed: u64::from(millimeters),
            });
        }
        self.eye_vertical_clamp_millimeters = Some(millimeters);
        Ok(self)
    }
    pub const fn offset_millimeters(self) -> [i32; 3] {
        self.offset_millimeters
    }
    pub const fn eye_vertical_clamp_millimeters(self) -> Option<u32> {
        self.eye_vertical_clamp_millimeters
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;

    #[test]
    fn strict_lower_and_inclusive_upper_partition_every_threshold() {
        for count in [65536_u32, 256] {
            for index in 0..count {
                let threshold = if count == 256 {
                    f64::from(index) / 255.0
                } else {
                    (f64::from(index) + 0.5) / 65536.0
                };
                for boundary in (0..=256).map(|step| f64::from(step) / 256.0).chain([
                    threshold.next_down(),
                    threshold,
                    threshold.next_up(),
                ]) {
                    let lower = threshold < boundary;
                    let upper = threshold >= boundary;
                    assert_ne!(lower, upper, "threshold={threshold}, boundary={boundary}");
                }
            }
        }
    }

    #[test]
    fn reference_validates_clamp_and_preserves_integer_offsets() {
        let reference = SceneFadeReference::new([i32::MIN, 17, i32::MAX]);
        assert_eq!(reference.offset_millimeters(), [i32::MIN, 17, i32::MAX]);
        assert_eq!(reference.eye_vertical_clamp_millimeters(), None);
        assert!(reference.with_eye_vertical_clamp(0).is_ok());
        assert!(reference.with_eye_vertical_clamp(1_000_000).is_ok());
        assert!(reference.with_eye_vertical_clamp(1_000_001).is_err());
    }
}
