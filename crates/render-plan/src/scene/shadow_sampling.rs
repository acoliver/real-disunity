use super::{SceneValidationError, effects::require_range};

/// Directional comparison footprint, in shadow-map texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneShadowFilter {
    /// Uniform grid of bilinear comparisons with receiver-plane correction.
    Grid { radius: u32 },
    /// Four neighbouring texels, individually corrected and bilinearly weighted.
    Bilinear2,
}

/// Retained directional caster and receiver policy, expressed in micro-units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneShadowSampling {
    /// Raster depth slope multiplier within `0..=1_000_000` millionths.
    /// Adds slope times `max(abs(dz/dx), abs(dz/dy))` to forward caster depth.
    pub caster_slope_bias_micro: u32,
    /// Comparison footprint shared by directional receivers.
    pub filter: SceneShadowFilter,
    /// Visibility blend weight within `0..=1_000_000`, applied to direct sunlight.
    pub strength_micro: u32,
}

impl Default for SceneShadowSampling {
    fn default() -> Self {
        Self {
            caster_slope_bias_micro: 0,
            filter: SceneShadowFilter::Grid { radius: 1 },
            strength_micro: 1_000_000,
        }
    }
}

impl SceneShadowSampling {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        require_range(
            "shadow caster slope bias_micro",
            u64::from(self.caster_slope_bias_micro),
            0,
            1_000_000,
        )?;
        require_range(
            "shadow strength_micro",
            u64::from(self.strength_micro),
            0,
            1_000_000,
        )?;
        if let SceneShadowFilter::Grid { radius } = self.filter {
            require_range("shadow grid radius", u64::from(radius), 1, 4)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_bounds() {
        let defaults = SceneShadowSampling::default();
        assert_eq!(defaults.caster_slope_bias_micro, 0);
        assert_eq!(defaults.filter, SceneShadowFilter::Grid { radius: 1 });
        assert_eq!(defaults.strength_micro, 1_000_000);
        for invalid in [
            SceneShadowSampling {
                strength_micro: 1_000_001,
                ..defaults
            },
            SceneShadowSampling {
                caster_slope_bias_micro: 1_000_001,
                ..defaults
            },
            SceneShadowSampling {
                filter: SceneShadowFilter::Grid { radius: 0 },
                ..defaults
            },
            SceneShadowSampling {
                filter: SceneShadowFilter::Grid { radius: 5 },
                ..defaults
            },
        ] {
            assert!(invalid.validate().is_err());
        }
        SceneShadowSampling {
            caster_slope_bias_micro: 3_000,
            strength_micro: 800_000,
            filter: SceneShadowFilter::Bilinear2,
        }
        .validate()
        .unwrap();
    }
}
