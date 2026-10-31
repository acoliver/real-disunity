/// Normal used only to offset the water's directional-shadow lookup position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneWaterShadowOffset {
    #[default]
    ShadingNormal,
    GeometricNormal,
    None,
}

/// Water-only directional-shadow response. The neutral value preserves stock bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneWaterShadowResponse {
    pub offset: SceneWaterShadowOffset,
    /// Minimum direct diffuse visibility in millionths (`0..=1_000_000`).
    pub diffuse_floor_micro: u32,
    /// Blend visibility toward one using the existing background tint weight,
    /// or the clamped one-metre refraction-depth ramp when tint is disabled.
    pub depth_relief: bool,
    /// Multiply the limited solar disk by strength-adjusted raw visibility again.
    pub disk_raw_visibility: bool,
}

impl SceneWaterShadowResponse {
    pub fn validate(self) -> Result<(), super::SceneValidationError> {
        super::effects::require_range(
            "water shadow diffuse floor",
            u64::from(self.diffuse_floor_micro),
            0,
            1_000_000,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_floor_is_validated_before_scene_allocation() {
        assert_eq!(
            SceneWaterShadowResponse::default().offset,
            SceneWaterShadowOffset::ShadingNormal
        );
        assert_eq!(
            super::super::SceneWaterSettings::STOCK.shadow_response,
            None
        );
        for floor in [0, 500_000, 1_000_000, 1_000_001, u32::MAX] {
            let mut settings = super::super::SceneWaterSettings::STOCK;
            settings.shadow_response = Some(SceneWaterShadowResponse {
                diffuse_floor_micro: floor,
                ..Default::default()
            });
            assert_eq!(settings.validate_refraction().is_ok(), floor <= 1_000_000);
        }
    }
}
