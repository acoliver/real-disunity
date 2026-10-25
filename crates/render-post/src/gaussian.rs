//! CPU-generated normalized Gaussian taps and separable bloom policy.
use crate::PostError;

pub const MAX_GAUSSIAN_RADIUS: u32 = 64;
pub const MAX_BLOOM_LEVELS: usize = 8;
// WGSL storage ABI: centre plus 32 adjacent-tap pairs.
pub(crate) const GAUSSIAN_TAP_CAPACITY: usize = 33;
pub(crate) const GAUSSIAN_WEIGHT_BYTES: usize = GAUSSIAN_TAP_CAPACITY * size_of::<[f32; 2]>();

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaussianLevel {
    /// Discrete support radius in this level's texels; zero is an identity blur.
    pub radius: u32,
    pub tint: [f32; 3],
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BloomKernel {
    #[default]
    Tent,
    /// Only the first `Bloom::levels` entries are used.
    SeparablePyramid([GaussianLevel; MAX_BLOOM_LEVELS]),
}

impl BloomKernel {
    pub(crate) fn validate(self, count: u32) -> Result<(), PostError> {
        if let Self::SeparablePyramid(levels) = self {
            for level in levels.iter().take(usize::try_from(count).unwrap()) {
                if level.radius > MAX_GAUSSIAN_RADIUS
                    || level
                        .tint
                        .iter()
                        .chain([&level.weight])
                        .any(|v| !v.is_finite() || *v < 0.0 || *v > 64.0)
                {
                    return Err(PostError::UnsupportedProfile("Gaussian bloom level"));
                }
            }
        }
        Ok(())
    }
}

/// One positive-side bilinear fetch, mirrored about the centre by the shader.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaussianTap {
    pub offset: f32,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GaussianWeights {
    pub center: f32,
    pub taps: Vec<GaussianTap>,
}

/// Truncated Gaussian with sigma = radius / 3. Radius zero has unit centre weight.
pub fn gaussian_weights(radius: u32) -> Result<GaussianWeights, PostError> {
    Ok(fold_weights(&discrete_weights(radius)?))
}

fn fold_weights(weights: &[f32]) -> GaussianWeights {
    let taps = weights[1..]
        .chunks(2)
        .enumerate()
        .map(|(index, pair)| {
            let first = f32::from(u16::try_from(index * 2 + 1).unwrap());
            let second = pair.get(1).copied().unwrap_or(0.0);
            let weight = pair[0] + second;
            GaussianTap {
                offset: first + second / weight,
                weight,
            }
        })
        .collect();
    GaussianWeights {
        center: weights[0],
        taps,
    }
}

pub(crate) fn discrete_weights(radius: u32) -> Result<Vec<f32>, PostError> {
    if radius > MAX_GAUSSIAN_RADIUS {
        return Err(PostError::UnsupportedProfile("Gaussian bloom radius"));
    }
    if radius == 0 {
        return Ok(vec![1.0]);
    }
    let sigma = f32::from(u16::try_from(radius).unwrap()) / 3.0;
    Ok(normalized_weights(radius, sigma))
}

fn normalized_weights(radius: u32, sigma: f32) -> Vec<f32> {
    let mut weights: Vec<_> = (0..=radius)
        .map(|i| {
            let distance = f32::from(u16::try_from(i).unwrap()) / sigma;
            (-0.5 * distance * distance).exp()
        })
        .collect();
    let sum = weights[0] + 2.0 * weights[1..].iter().sum::<f32>();
    for weight in &mut weights {
        *weight /= sum;
    }
    weights
}

/// Ceiling-half extents beginning at half resolution, with one terminal 1x1 level.
pub fn gaussian_extents(size: [u32; 2], count: u32) -> Result<Vec<[u32; 2]>, PostError> {
    if size.contains(&0) {
        return Err(PostError::EmptyExtent);
    }
    if !(1..=u32::try_from(MAX_BLOOM_LEVELS).unwrap()).contains(&count) {
        return Err(PostError::UnsupportedProfile("Gaussian bloom level count"));
    }
    let mut next = size.map(|side| side.div_ceil(2));
    let mut extents = Vec::new();
    for level in 0..count {
        extents.push(next);
        if next == [1, 1] && level + 1 < count {
            return Err(PostError::UnsupportedProfile(
                "Gaussian bloom levels exceed extent",
            ));
        }
        next = next.map(|side| side.div_ceil(2));
    }
    Ok(extents)
}

#[cfg(test)]
#[path = "gaussian_tests.rs"]
mod tests;

impl crate::GaussianKernelRule {
    /// Discrete support and sigma for one axis. Radius zero is an identity.
    pub fn parameters(self, radius: u32, extent: u32) -> Result<(u32, f32), PostError> {
        if radius > MAX_GAUSSIAN_RADIUS {
            return Err(PostError::UnsupportedProfile("Gaussian bloom radius"));
        }
        let (support, sigma) = match self {
            Self::RadiusThird => (radius, f32::from(u16::try_from(radius).unwrap()) / 3.0),
            Self::AxisClampedPairs => {
                let pairs = radius.min(extent.max(1)).div_ceil(2);
                let support = 2 * pairs;
                (
                    support,
                    (f32::from(u16::try_from(support).unwrap()) + 0.5) / 3.0,
                )
            }
        };
        Ok((support, if radius == 0 { 0.0 } else { sigma }))
    }
    pub fn weights(self, radius: u32, extent: u32) -> Result<GaussianWeights, PostError> {
        let (support, sigma) = self.parameters(radius, extent)?;
        if support == 0 {
            return gaussian_weights(0);
        }
        Ok(fold_weights(&normalized_weights(support, sigma)))
    }
}
