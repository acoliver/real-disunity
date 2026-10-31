use super::effects::require_range;
use super::{EFFECTS_MICRO_SCALE, SceneValidationError};

/// Deterministic ambient occlusion sampling policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSsaoKernel {
    /// Project samples from a view-normal oriented hemisphere.
    Hemisphere,
    /// Sample a screen-space spiral using averaged depth/normal footprints.
    Spiral,
    /// Half-resolution scalable ambient obscurance with filtered depth taps.
    Scalable {
        /// Spiral revolutions in micro-units, within 1..=64000000.
        turns_micro: u32,
        /// Response exponent in micro-units, within 1..=16000000.
        power_micro: u32,
    },
}

/// Depth-aware ambient occlusion reconstruction policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSsaoFilter {
    /// Full-resolution five-by-five depth-aware tent.
    DepthAwareBox,
    /// Horizontal then vertical nine-tap Gaussian, spaced two pixels apart.
    /// Depth differences at least one metre are excluded; accepted weights use
    /// an exponential sharpness of seven per metre.
    SeparableBilateral,
    /// Gaussian bilateral reconstruction for the scalable kernel.
    ScalableBilateral {
        /// Tap radius within 1..=16.
        radius: u32,
        /// Distance between taps within 1..=8 pixels.
        step: u32,
        /// Gaussian sigma divisor in micro-units, within 1..=16000000.
        /// Sigma is the radius divided by this value.
        sigma_scale_micro: u32,
        /// Depth exponent coefficient in micro-units per metre, at most 64000000.
        sharpness_micro: u32,
    },
}

/// Full-resolution reconstruction of the filtered screen AO buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneSsaoReceiver {
    /// Load the nearest AO texel without changing material composition.
    #[default]
    Nearest,
    /// Four taps weighted by positive view-depth differences in metres.
    DepthAwareGather,
}

/// Screen-space ambient occlusion parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSsao {
    /// Receiver reconstruction, independent of the AO kernel and filter.
    pub receiver: SceneSsaoReceiver,
    /// Sampled depth chain for the scalable kernel; other kernels require the default.
    pub depth_footprint: super::SceneSsaoDepthFootprint,
    /// Sampling algorithm and its metadata reduction contract.
    pub kernel: SceneSsaoKernel,
    /// Reconstruction filter, independent of sampling.
    pub filter: SceneSsaoFilter,
    /// Number of deterministic samples, within 1..=64.
    pub sample_count: u32,
    /// Surface self-occlusion bias in millimetres.
    pub bias_millimeters: u32,
    /// Occlusion sample radius in millimeters; strictly positive.
    pub radius_millimeters: u64,
    /// Occlusion strength in micro-units within `0..=2_000_000`.
    /// For Scalable, the enhancement blend is bounded to `[0, 1]`.
    /// Below strength two-thirds, obscurance before the square root is scaled
    /// by `1.5 * strength`, reaching zero continuously. At or above two-thirds,
    /// the enhancement and response equations are unchanged. Power is independent.
    pub intensity_micro: u64,
    /// Sample horizon limit in millidegrees within `0..=180_000`.
    pub horizon_angle_millidegrees: u32,
}

impl SceneSsao {
    /// Validates radius, intensity, and horizon bounds.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first out-of-bounds field.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.depth_footprint.validate()?;
        if !matches!(self.kernel, SceneSsaoKernel::Scalable { .. })
            && self.depth_footprint != super::SceneSsaoDepthFootprint::default()
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "ssao depth footprint requires scalable kernel",
            });
        }
        self.validate_policy()?;
        require_range("ssao sample_count", u64::from(self.sample_count), 1, 64)?;
        require_range(
            "ssao radius_millimeters",
            self.radius_millimeters,
            1,
            u64::MAX,
        )?;
        require_range(
            "ssao intensity_micro",
            self.intensity_micro,
            0,
            2 * EFFECTS_MICRO_SCALE,
        )?;
        require_range(
            "ssao horizon_angle_millidegrees",
            u64::from(self.horizon_angle_millidegrees),
            0,
            180_000,
        )
    }
    fn validate_policy(&self) -> Result<(), SceneValidationError> {
        match (self.kernel, self.filter) {
            (
                SceneSsaoKernel::Scalable {
                    turns_micro,
                    power_micro,
                },
                SceneSsaoFilter::ScalableBilateral {
                    radius,
                    step,
                    sigma_scale_micro,
                    sharpness_micro,
                },
            ) => {
                require_range(
                    "ssao scalable radius",
                    self.radius_millimeters,
                    1,
                    1_000_000,
                )?;
                require_range(
                    "ssao scalable bias",
                    u64::from(self.bias_millimeters),
                    0,
                    self.radius_millimeters - 1,
                )?;
                require_range("ssao turns", u64::from(turns_micro), 1, 64_000_000)?;
                require_range("ssao power", u64::from(power_micro), 1, 16_000_000)?;
                require_range("ssao blur radius", u64::from(radius), 1, 16)?;
                require_range("ssao blur step", u64::from(step), 1, 8)?;
                require_range(
                    "ssao blur sigma",
                    u64::from(sigma_scale_micro),
                    1,
                    16_000_000,
                )?;
                require_range(
                    "ssao blur sharpness",
                    u64::from(sharpness_micro),
                    0,
                    64_000_000,
                )?;
            }
            (SceneSsaoKernel::Scalable { .. }, _)
            | (_, SceneSsaoFilter::ScalableBilateral { .. }) => {
                require_range("ssao scalable kernel/filter pair", 0, 1, 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_is_validated_and_requires_scalable_sampling() {
        let mut policy = settings();
        for level_count in [0, 9, u32::MAX] {
            policy.depth_footprint.level_count = level_count;
            assert!(policy.validate().is_err());
        }
        policy.depth_footprint.level_count = 5;
        policy.depth_footprint.rounding = crate::SceneDownsampleRounding::Floor;
        assert!(policy.validate().is_ok());
        policy.kernel = SceneSsaoKernel::Hemisphere;
        policy.filter = SceneSsaoFilter::DepthAwareBox;
        assert!(policy.validate().is_err());
        policy.depth_footprint = crate::SceneSsaoDepthFootprint::default();
        assert!(policy.validate().is_ok());
    }
    fn settings() -> SceneSsao {
        SceneSsao {
            receiver: crate::SceneSsaoReceiver::Nearest,
            depth_footprint: crate::SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Scalable {
                turns_micro: 7_000_000,
                power_micro: 1_000_000,
            },
            filter: filter(4, 2, 1_500_000, 7_000_000),
            sample_count: 10,
            bias_millimeters: 70,
            radius_millimeters: 1000,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }
    }
    fn filter(
        radius: u32,
        step: u32,
        sigma_scale_micro: u32,
        sharpness_micro: u32,
    ) -> SceneSsaoFilter {
        SceneSsaoFilter::ScalableBilateral {
            radius,
            step,
            sigma_scale_micro,
            sharpness_micro,
        }
    }
    #[test]
    fn scalable_policy_rejects_each_invalid_parameter() {
        assert!(settings().validate().is_ok());
        for kernel in [
            SceneSsaoKernel::Scalable {
                turns_micro: 0,
                power_micro: 1_000_000,
            },
            SceneSsaoKernel::Scalable {
                turns_micro: 64_000_001,
                power_micro: 1_000_000,
            },
            SceneSsaoKernel::Scalable {
                turns_micro: 7_000_000,
                power_micro: 0,
            },
            SceneSsaoKernel::Scalable {
                turns_micro: 7_000_000,
                power_micro: 16_000_001,
            },
            SceneSsaoKernel::Hemisphere,
        ] {
            assert!(
                SceneSsao {
                    kernel,
                    ..settings()
                }
                .validate()
                .is_err()
            );
        }
    }
    #[test]
    fn scalable_filter_and_geometry_bounds_are_validated() {
        for filter in [
            filter(0, 2, 1_500_000, 7_000_000),
            filter(17, 2, 1_500_000, 7_000_000),
            filter(4, 0, 1_500_000, 7_000_000),
            filter(4, 9, 1_500_000, 7_000_000),
            filter(4, 2, 0, 7_000_000),
            filter(4, 2, 16_000_001, 7_000_000),
            filter(4, 2, 1_500_000, 64_000_001),
            SceneSsaoFilter::DepthAwareBox,
        ] {
            assert!(
                SceneSsao {
                    filter,
                    ..settings()
                }
                .validate()
                .is_err()
            );
        }
        for radius_millimeters in [0, 1_000_001, u64::MAX] {
            assert!(
                SceneSsao {
                    radius_millimeters,
                    ..settings()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            SceneSsao {
                bias_millimeters: 1000,
                ..settings()
            }
            .validate()
            .is_err()
        );
    }
}
