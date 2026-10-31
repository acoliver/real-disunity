//! CPU admission and extent contracts for post processing.

pub const MAX_GAUSSIAN_RADIUS: u32 = 64;

/// Admission errors shared by CPU planning and GPU post processing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PostContractError {
    #[error("unsupported frame profile: {0}")]
    UnsupportedProfile(&'static str),
    #[error("post target extent is empty")]
    EmptyExtent,
}

/// Internal/output ratio in millionths, from 0.5 through 1.0 inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderScale(u32);
impl RenderScale {
    pub fn new(millionths: u32) -> Result<Self, PostContractError> {
        if !(500_000..=1_000_000).contains(&millionths) {
            return Err(PostContractError::UnsupportedProfile(
                "render scale must be 500000..=1000000 millionths",
            ));
        }
        Ok(Self(millionths))
    }
    /// Round to nearest integer, with halfway values rounded up.
    pub fn extent(self, output: [u32; 2]) -> Result<[u32; 2], PostContractError> {
        if output.contains(&0) {
            return Err(PostContractError::EmptyExtent);
        }
        Ok(output.map(|side| {
            u32::try_from((u64::from(side) * u64::from(self.0) + 500_000) / 1_000_000)
                .expect("validated scale cannot exceed the output extent")
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpscaleMode {
    Bilinear,
    Easu,
    /// Temporal reconstruction onto the output grid, with TAA as the only accumulator.
    ///
    /// It needs the scene HDR profile, a TAA resolve and one sample per pixel; see
    /// `worldrunner_render_post::HdrPostSettings::validate_frame`. At equal extents it is the native TAA path.
    Temporal,
}

/// Spatial reconstruction of display-referred RGB. `None` disables RCAS exactly.
/// RCAS sharpness is in stops: zero is maximum; the supported range is 0..=2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpscaleSettings {
    pub mode: UpscaleMode,
    pub sharpness: Option<f32>,
}
impl UpscaleSettings {
    pub fn validate(self) -> Result<(), PostContractError> {
        if self
            .sharpness
            .is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v))
        {
            return Err(PostContractError::UnsupportedProfile(
                "RCAS sharpness must be finite and within 0..=2 stops",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_rounding_and_rejections() {
        for scale in [0, 499_999, 1_000_001, u32::MAX] {
            assert!(RenderScale::new(scale).is_err());
        }
        assert_eq!(
            RenderScale::new(500_000).unwrap().extent([1, 1]),
            Ok([1, 1])
        );
        assert_eq!(
            RenderScale::new(770_000).unwrap().extent([50, 26]),
            Ok([39, 20])
        );
        assert_eq!(
            RenderScale::new(1_000_000).unwrap().extent([33, 17]),
            Ok([33, 17])
        );
        assert_eq!(
            RenderScale::new(500_000).unwrap().extent([0, 1]),
            Err(PostContractError::EmptyExtent)
        );
    }
    #[test]
    fn sharpness_validation() {
        for sharpness in [f32::NAN, f32::INFINITY, -0.01, 2.01] {
            assert!(
                UpscaleSettings {
                    mode: UpscaleMode::Easu,
                    sharpness: Some(sharpness)
                }
                .validate()
                .is_err()
            );
        }
        for sharpness in [None, Some(0.0), Some(2.0)] {
            assert!(
                UpscaleSettings {
                    mode: UpscaleMode::Easu,
                    sharpness
                }
                .validate()
                .is_ok()
            );
        }
    }
}
