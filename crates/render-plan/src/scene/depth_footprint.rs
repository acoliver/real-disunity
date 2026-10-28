use super::{SceneValidationError, effects::require_range};

/// Rounding for downsampled axes. Every nonzero axis remains at least one texel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SceneDownsampleRounding {
    #[default]
    Ceil,
    Floor,
}
impl SceneDownsampleRounding {
    pub fn half_extent(self, size: [u32; 2]) -> [u32; 2] {
        size.map(|side| match self {
            Self::Ceil => side.div_ceil(2).max(1),
            Self::Floor => (side / 2).max(1),
        })
    }
}

/// Averaged half-resolution depth levels used by scalable ambient obscurance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSsaoDepthFootprint {
    /// Maximum sampled levels, within 1..=8. Stop earlier at a one-texel image.
    pub level_count: u32,
    /// Applies to the initial half-resolution target and every subsequent level.
    pub rounding: SceneDownsampleRounding,
}
impl Default for SceneSsaoDepthFootprint {
    fn default() -> Self {
        Self {
            level_count: 8,
            rounding: SceneDownsampleRounding::Ceil,
        }
    }
}
impl SceneSsaoDepthFootprint {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        require_range("ssao depth levels", u64::from(self.level_count), 1, 8)
    }
    /// Requires nonzero source dimensions and a validated level count.
    pub fn extents(self, mut size: [u32; 2]) -> Vec<[u32; 2]> {
        assert!(!size.contains(&0), "nonzero depth footprint extent");
        self.validate().expect("validated depth footprint");
        let mut extents = Vec::new();
        for _ in 0..self.level_count {
            size = self.rounding.half_extent(size);
            extents.push(size);
            if size == [1, 1] {
                break;
            }
        }
        extents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_match_independent_integer_division_oracle() {
        for rounding in [
            SceneDownsampleRounding::Ceil,
            SceneDownsampleRounding::Floor,
        ] {
            for source in [[1, 1], [1, 493], [493, 1], [897, 493], [1164, 640]] {
                let policy = SceneSsaoDepthFootprint {
                    level_count: 5,
                    rounding,
                };
                let actual = policy.extents(source);
                for (index, extent) in actual.iter().enumerate() {
                    let divisor = 1u32 << (index + 1);
                    let expected = source.map(|axis| match rounding {
                        SceneDownsampleRounding::Ceil => ((axis - 1) / divisor + 1).max(1),
                        SceneDownsampleRounding::Floor => (axis / divisor).max(1),
                    });
                    assert_eq!(*extent, expected);
                }
                assert!(actual.len() <= 5);
            }
        }
        let floor = SceneSsaoDepthFootprint {
            level_count: 5,
            rounding: SceneDownsampleRounding::Floor,
        };
        assert_eq!(
            floor.extents([897, 493]),
            [[448, 246], [224, 123], [112, 61], [56, 30], [28, 15]]
        );
        assert_eq!(
            SceneSsaoDepthFootprint::default().extents([513, 257]),
            [
                [257, 129],
                [129, 65],
                [65, 33],
                [33, 17],
                [17, 9],
                [9, 5],
                [5, 3],
                [3, 2]
            ]
        );
    }

    #[test]
    fn level_limits_are_validated() {
        for level_count in [0, 9, u32::MAX] {
            assert!(
                SceneSsaoDepthFootprint {
                    level_count,
                    ..SceneSsaoDepthFootprint::default()
                }
                .validate()
                .is_err()
            );
        }
        for level_count in 1..=8 {
            assert!(
                SceneSsaoDepthFootprint {
                    level_count,
                    ..SceneSsaoDepthFootprint::default()
                }
                .validate()
                .is_ok()
            );
        }
    }
}
