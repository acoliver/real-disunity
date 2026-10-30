//! Linear material composition independent of source texture storage.

use super::selection::PageKey;
use glam::Vec3;
use thiserror::Error;
use worldrunner_render_plan::TerrainPageSettings;

/// One decoded, sampled material layer at a terrain location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerSample {
    /// Linear RGBA8, not sRGB-encoded values.
    pub color: [u8; 4],
    /// Signed direction in the common terrain material frame, normalized before blending.
    /// X follows unrotated U (east), Y follows unrotated V (north), and Z points out.
    /// Source texel columns and rows increase U and V, without a vertical flip.
    /// Decode normalized source channels as `2 * value - 1`, then inverse-rotate
    /// each appearance's XY direction before supplying this already-unrotated value.
    pub normal: [i16; 3],
    /// Perceptual roughness in UNORM8.
    pub roughness: u8,
    /// Nonnegative coverage. At least one sample must have positive coverage.
    pub weight: u16,
}

/// The two packed four-byte page products, both sampled as linear UNORM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialTexel {
    pub color: [u8; 4],
    /// Normal XYZ mapped from [-1, 1] to [0, 255], then roughness.
    pub normal_roughness: [u8; 4],
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CompositionError {
    #[error("composition requires between one and 256 layers")]
    LayerCount,
    #[error("material coverage must have positive total weight")]
    EmptyCoverage,
    #[error("a contributing normal or the blended normal has zero length")]
    Normal,
    #[error("material image dimensions and payload disagree or overflow")]
    ImageExtent,
}

/// Validated row-major image, including its source-composed gutters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialImage {
    extent: [u32; 2],
    texels: Vec<MaterialTexel>,
}

impl MaterialImage {
    pub fn new(extent: [u32; 2], texels: Vec<MaterialTexel>) -> Result<Self, CompositionError> {
        let count = u64::from(extent[0]) * u64::from(extent[1]);
        if extent.contains(&0) || usize::try_from(count).ok() != Some(texels.len()) {
            return Err(CompositionError::ImageExtent);
        }
        Ok(Self { extent, texels })
    }
    pub const fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub fn texels(&self) -> &[MaterialTexel] {
        &self.texels
    }
}

/// Exact integer color/roughness blending and normalized weighted tangent normals.
pub fn compose_texel(samples: &[LayerSample]) -> Result<MaterialTexel, CompositionError> {
    if samples.is_empty() || samples.len() > 256 {
        return Err(CompositionError::LayerCount);
    }
    let mut sum = [0_u32; 5];
    let mut weight = 0_u32;
    let mut normal = Vec3::ZERO;
    for sample in samples.iter().filter(|sample| sample.weight > 0) {
        let direction = Vec3::from_array(sample.normal.map(f32::from));
        let direction = direction.try_normalize().ok_or(CompositionError::Normal)?;
        normal += direction * f32::from(sample.weight);
        let coverage = u32::from(sample.weight);
        weight += coverage;
        for (total, value) in sum
            .iter_mut()
            .zip(sample.color.into_iter().chain([sample.roughness]))
        {
            *total += u32::from(value) * coverage;
        }
    }
    if weight == 0 {
        return Err(CompositionError::EmptyCoverage);
    }
    let direction = normal.try_normalize().ok_or(CompositionError::Normal)?;
    let values =
        sum.map(|value| u8::try_from((value + weight / 2) / weight).expect("weighted UNORM8 mean"));
    Ok(MaterialTexel {
        color: [values[0], values[1], values[2], values[3]],
        normal_roughness: [
            pack_normal(direction.x),
            pack_normal(direction.y),
            pack_normal(direction.z),
            values[4],
        ],
    })
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "normalized clamped UNORM conversion"
)]
pub(super) fn pack_normal(value: f32) -> u8 {
    ((value * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Composes interior and gutter texel centers in global normalized terrain coordinates.
///
/// Coordinates outside [0, 1] occur at the outer heightfield boundary. The source
/// sampler must implement its declared boundary policy there. Internal page
/// boundaries must sample neighboring content rather than clamp to this page.
pub fn compose_page(
    settings: TerrainPageSettings,
    key: PageKey,
    mut sample: impl FnMut([f64; 2]) -> Vec<LayerSample>,
) -> Result<MaterialImage, CompositionError> {
    let extent = settings.extent();
    let count = usize::try_from(u64::from(extent[0]) * u64::from(extent[1]))
        .map_err(|_| CompositionError::ImageExtent)?;
    let mut texels = Vec::with_capacity(count);
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            texels.push(compose_texel(&sample(source_uv(settings, key, [x, y])))?);
        }
    }
    MaterialImage::new(extent, texels)
}

pub(super) fn source_uv(settings: TerrainPageSettings, key: PageKey, texel: [u32; 2]) -> [f64; 2] {
    let policy = settings.parts();
    let scale = f64::from(1_u32 << key.depth());
    std::array::from_fn(|axis| {
        let local = (f64::from(texel[axis]) + 0.5 - f64::from(policy.border))
            / f64::from(policy.interior[axis]);
        (f64::from(key.coordinate()[axis]) + local) / scale
    })
}
