use super::{SceneValidationError, SceneWaterSettings, effects::require_range};

/// Edge attenuation when the spatial noise frequency is zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneWaterZeroFrequency {
    #[default]
    Skip,
    ConstantHalf,
}

/// Reflected environment controls, separate from the analytic specular amplitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterEnvironment {
    /// Linear environment amplitude, in millionths (`0..=10_000_000`).
    pub amplitude_micro: u32,
    /// Simplex frequency per metre, in millionths.
    pub edge_frequency_micro: u32,
    /// Attenuation policy at zero frequency; Skip preserves the default output.
    pub zero_frequency: SceneWaterZeroFrequency,
    /// Attenuate reflected radiance at shallow depths by `clamp(depth * 20, 0, 1)`.
    pub depth_self_occlusion: bool,
}

/// Analytic solar highlight controls, applied before the material's squared gain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterSunDisk {
    /// Highlight irradiance multiplier, in millionths (`0..=10_000_000`).
    pub amplitude_micro: u32,
    /// Linear luminance ceiling, in millionths; zero suppresses the highlight.
    pub peak_micro: u32,
    /// Angular radius in microradians (`1..=100_000`); default is 2700.
    pub angular_radius_microradians: u32,
}
impl Default for SceneWaterSunDisk {
    fn default() -> Self {
        Self {
            amplitude_micro: 1_000_000,
            peak_micro: 1_000_000,
            angular_radius_microradians: 2700,
        }
    }
}

/// Addressing of the flow-seed lattice, independent of normal-map repetition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneWaterSeedCoordinates {
    #[default]
    Repeat,
    /// Samples span both domain endpoints and clamp outside the domain.
    /// Requires at least two texels on each axis.
    EndpointClamped,
}

/// Planar seed coordinates independent of the normal map's world-space repetition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterFlowCoordinates {
    pub origin_millimeters: [i32; 2],
    /// Positive domain extents along east and north; periods in Repeat mode.
    pub tile_millimeters: [u32; 2],
    pub mode: SceneWaterSeedCoordinates,
}

/// Vertex-sampled planar scene wind, with a spatial and temporal perturbation band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterWind {
    pub field: super::ScenePlanarWindField,
    pub band: super::SceneWindBand,
    /// Blend from unchanged authored flow to wind-shaped seeds, in millionths.
    pub amplitude_micro: u32,
}

pub(super) fn validate(settings: &SceneWaterSettings) -> Result<(), SceneValidationError> {
    if let Some(environment) = settings.environment {
        require_range(
            "water environment amplitude",
            u64::from(environment.amplitude_micro),
            0,
            10_000_000,
        )?;
        require_range(
            "water edge frequency",
            u64::from(environment.edge_frequency_micro),
            0,
            100_000_000,
        )?;
    }
    if let Some(disk) = settings.sun_disk {
        require_range(
            "water sun angular radius",
            u64::from(disk.angular_radius_microradians),
            1,
            100_000,
        )?;
        require_range(
            "water sun amplitude",
            u64::from(disk.amplitude_micro),
            0,
            10_000_000,
        )?;
    }
    if let Some(coordinates) = settings.flow_coordinates
        && (coordinates.tile_millimeters.contains(&0) || settings.flow.is_none())
    {
        return Err(SceneValidationError::UnsupportedInput {
            part: "water flow coordinates require positive tiles and a flow map",
        });
    }
    if let Some(wind) = settings.wind {
        require_range(
            "water wind amplitude",
            u64::from(wind.amplitude_micro),
            0,
            1_000_000,
        )?;
        if settings.flow.is_none() || settings.flow_period_milliseconds.is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water wind requires flow seeds and a two-phase period",
            });
        }
        if wind.field.modifiers().iter().any(Option::is_some) {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water wind does not apply foliage force modifiers",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_modes_require_flow_and_positive_domain_extents() {
        assert_eq!(
            SceneWaterSeedCoordinates::default(),
            SceneWaterSeedCoordinates::Repeat
        );
        for mode in [
            SceneWaterSeedCoordinates::Repeat,
            SceneWaterSeedCoordinates::EndpointClamped,
        ] {
            for extent in [[0, 1000], [1000, 0], [1000, 1000]] {
                let mut settings = SceneWaterSettings {
                    flow_coordinates: Some(SceneWaterFlowCoordinates {
                        origin_millimeters: [-5000, 1234],
                        tile_millimeters: extent,
                        mode,
                    }),
                    ..SceneWaterSettings::STOCK
                };
                assert!(validate(&settings).is_err());
                settings.flow = Some(super::super::TextureHandle::from_raw(0));
                assert_eq!(validate(&settings).is_ok(), !extent.contains(&0));
            }
        }
    }

    #[test]
    fn disk_radii_validate_in_microradians() {
        assert_eq!(
            SceneWaterSunDisk::default().angular_radius_microradians,
            2700
        );
        let surface = super::super::SceneSurfaceDisk::new(1_000_000, 1_000_000).unwrap();
        assert_eq!(surface.angular_radius_microradians(), 2700);
        for radius in [0, 1, 2700, 17_453, 100_000, 100_001, u32::MAX] {
            let settings = SceneWaterSettings {
                sun_disk: Some(SceneWaterSunDisk {
                    angular_radius_microradians: radius,
                    ..Default::default()
                }),
                ..SceneWaterSettings::STOCK
            };
            let valid = (1..=100_000).contains(&radius);
            assert_eq!(validate(&settings).is_ok(), valid);
            assert_eq!(
                surface.with_angular_radius_microradians(radius).is_ok(),
                valid
            );
        }
    }
}
