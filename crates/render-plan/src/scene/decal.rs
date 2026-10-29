//! Box projection descriptions in renderer scene space.
use super::{MaterialHandle, RenderScene, SceneTransform, SceneValidationError};

/// Positive local axis facing the receiving surface's outward normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneDecalAxis {
    X,
    Y,
    Z,
}

/// Receiver categories. Water and background geometry never receive decals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDecalReceivers(u32);
impl SceneDecalReceivers {
    pub const MESH: Self = Self(1);
    pub const TERRAIN: Self = Self(2);
    pub const ALL: Self = Self(3);
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Channels modified by a decal. Emission is not a decal-blendable channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDecalWriteMask(u32);
impl SceneDecalWriteMask {
    pub const NONE: Self = Self(0);
    pub const ALBEDO: Self = Self(1);
    pub const NORMAL: Self = Self(2);
    pub const ROUGHNESS_METALNESS: Self = Self(4);
    pub const ALL: Self = Self(7);
    /// Raw masks are validated when their decal enters a scene.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
}
impl Default for SceneDecalWriteMask {
    fn default() -> Self {
        Self::ALL
    }
}

/// Opacity response to the cosine between projection and receiver normals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneDecalAngle {
    #[default]
    Linear,
    /// Positive threshold cosine and exponent, both in millionths.
    Power {
        threshold_micro: u32,
        exponent_micro: u32,
    },
}

/// Coordinate frame for correcting decoded detail normals onto the receiver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneDecalNormalFrame {
    /// Correct in the placement's projection basis, then return to world space.
    #[default]
    Projection,
    /// Correct onto the world-space geometric normal, without placement rotation.
    /// At exactly negative Z, use an X-axis half turn: `(dx, -dy, -dz)`.
    /// Every nonsingular receiver uses the reference basis without a pole threshold.
    ReceiverWorld,
}

/// Sampling controls independent of the projection hull.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDecalMapping {
    /// Applied after the projected local coordinates are shifted into [0, 1].
    pub uv_scale_micro: [i32; 2],
    pub uv_offset_micro: [i32; 2],
    /// Power replaces the linear cutoff and fade interval.
    pub angle: SceneDecalAngle,
    /// Uses the absolute projection cosine, accepting both sides.
    pub double_sided: bool,
    /// Used for both shading and normal metadata.
    pub normal_frame: SceneDecalNormalFrame,
}
impl Default for SceneDecalMapping {
    fn default() -> Self {
        Self {
            uv_scale_micro: [1_000_000; 2],
            uv_offset_micro: [0; 2],
            angle: SceneDecalAngle::Linear,
            double_sided: false,
            normal_frame: SceneDecalNormalFrame::Projection,
        }
    }
}

/// A centred unit box, stretched by its dimensions before its placement transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDecal {
    /// Higher priorities win visible-set overflow; ties use distance then stable ID.
    pub priority: i32,
    pub transform: SceneTransform,
    pub dimensions_millimeters: [u32; 3],
    pub material: MaterialHandle,
    pub axis: SceneDecalAxis,
    /// Local distance from the box boundary to full opacity, in millionths.
    pub edge_fade_micro: u32,
    /// Cosine below which projection is rejected, in millionths.
    pub angle_cutoff_micro: u32,
    /// Cosine interval over which opacity rises from zero to one.
    pub angle_fade_micro: u32,
    pub receivers: SceneDecalReceivers,
    pub mapping: SceneDecalMapping,
    pub write_mask: SceneDecalWriteMask,
    /// Tests angle-faded texture alpha before edge fade and material opacity.
    /// Equality passes; cutoff is in millionths within `0..=1_000_000`.
    pub alpha_test_cutoff_micro: Option<u32>,
}
impl SceneDecal {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.transform.validate()?;
        if self.write_mask.bits() & !SceneDecalWriteMask::ALL.bits() != 0
            || self
                .alpha_test_cutoff_micro
                .is_some_and(|cutoff| cutoff > 1_000_000)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "decal write mask or alpha test",
            });
        }
        if let SceneDecalAngle::Power {
            threshold_micro,
            exponent_micro,
        } = self.mapping.angle
            && (threshold_micro == 0 || threshold_micro > 1_000_000 || exponent_micro == 0)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "decal power fade",
            });
        }
        if self.dimensions_millimeters.contains(&0)
            || self.edge_fade_micro > 500_000
            || self.angle_cutoff_micro > 1_000_000
            || self.angle_fade_micro > 1_000_000 - self.angle_cutoff_micro
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "decal dimensions or fade",
            });
        }
        Ok(())
    }
}
impl RenderScene {
    /// Replaces the ordered projection list. An empty list removes every decal.
    pub fn with_decals(&self, decals: Vec<SceneDecal>) -> Result<Self, SceneValidationError> {
        let scene = Self {
            decals: decals.into_boxed_slice(),
            ..self.clone()
        };
        scene.validate()?;
        Ok(scene)
    }
    pub fn decals(&self) -> &[SceneDecal] {
        &self.decals
    }
    pub(super) fn validate_decals(&self) -> Result<(), SceneValidationError> {
        for decal in &self.decals {
            decal.validate()?;
            super::require_handle(
                "decal",
                decal.material.raw(),
                "material",
                super::table_limit(self.resources.materials().len(), "materials")?,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn decal() -> SceneDecal {
        SceneDecal {
            priority: 0,
            transform: SceneTransform {
                position_millimeters: [0; 3],
                rotation: super::super::SceneRotation::IDENTITY,
                scale_millionths: 1_000_000,
            },
            dimensions_millimeters: [1000; 3],
            material: MaterialHandle::from_raw(0),
            axis: SceneDecalAxis::Z,
            edge_fade_micro: 0,
            angle_cutoff_micro: 0,
            angle_fade_micro: 0,
            receivers: SceneDecalReceivers::ALL,
            mapping: SceneDecalMapping::default(),
            write_mask: SceneDecalWriteMask::default(),
            alpha_test_cutoff_micro: None,
        }
    }
    #[test]
    fn validates_write_mask_and_alpha_test_bounds() {
        assert_eq!(SceneDecalWriteMask::default(), SceneDecalWriteMask::ALL);
        for bits in 0..8 {
            let mut d = decal();
            d.write_mask = SceneDecalWriteMask::from_bits(bits);
            assert!(d.validate().is_ok());
        }
        for bits in [8, 15, u32::MAX] {
            let mut d = decal();
            d.write_mask = SceneDecalWriteMask::from_bits(bits);
            assert!(d.validate().is_err());
        }
        for (cutoff, valid) in [(0, true), (1_000_000, true), (1_000_001, false)] {
            let mut d = decal();
            d.alpha_test_cutoff_micro = Some(cutoff);
            assert_eq!(d.validate().is_ok(), valid);
        }
    }
    #[test]
    fn validates_volume_and_fade_intervals() {
        let mut d = decal();
        assert!(d.validate().is_ok());
        d.dimensions_millimeters[1] = 0;
        assert!(d.validate().is_err());
        d = decal();
        d.edge_fade_micro = 500_001;
        assert!(d.validate().is_err());
        d = decal();
        d.angle_cutoff_micro = 800_000;
        d.angle_fade_micro = 200_001;
        assert!(d.validate().is_err());
        d = decal();
        d.transform.scale_millionths = 0;
        assert!(d.validate().is_err());
    }
    #[test]
    fn mapping_defaults_preserve_linear_single_sided_unit_uvs() {
        let mapping = SceneDecalMapping::default();
        assert_eq!(mapping.uv_scale_micro, [1_000_000; 2]);
        assert_eq!(mapping.uv_offset_micro, [0; 2]);
        assert_eq!(mapping.angle, SceneDecalAngle::Linear);
        assert!(!mapping.double_sided);
        assert_eq!(mapping.normal_frame, SceneDecalNormalFrame::Projection);
        for frame in [
            SceneDecalNormalFrame::Projection,
            SceneDecalNormalFrame::ReceiverWorld,
        ] {
            let mut d = decal();
            d.mapping.normal_frame = frame;
            assert!(d.validate().is_ok());
        }
        for (threshold_micro, exponent_micro, valid) in [
            (0, 2_400_000, false),
            (1_000_001, 2_400_000, false),
            (700_000, 0, false),
            (700_000, 2_400_000, true),
        ] {
            let mut d = decal();
            d.mapping.angle = SceneDecalAngle::Power {
                threshold_micro,
                exponent_micro,
            };
            assert_eq!(d.validate().is_ok(), valid);
        }
    }
}
