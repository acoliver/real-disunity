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
}
impl SceneDecal {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.transform.validate()?;
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
}
