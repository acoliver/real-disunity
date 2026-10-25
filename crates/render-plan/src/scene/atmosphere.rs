//! Optional scene-linear atmospheric background. Quantities retain integer scene units.
use super::effects::require_range;
use super::{SceneDirection, SceneValidationError, TextureHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneHeightFog {
    /// Extinction at base height, in millionths per metre.
    pub density_micro_per_meter: u32,
    pub falloff_micro_per_meter: u32,
    pub base_height_millimeters: i32,
    pub start_distance_millimeters: u32,
    /// Linear RGB radiance, not sRGB.
    pub radiance_micro: [u32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCelestialDisc {
    pub direction: SceneDirection,
    /// Angular radius, not diameter. Strictly smaller than a hemisphere.
    pub angular_radius_millidegrees: u32,
    /// Rotation about the viewing direction, counterclockwise in the disc plane.
    pub rotation_millidegrees: u32,
    /// Optional RGBA disc texture; alpha is coverage.
    pub texture: Option<TextureHandle>,
    /// Linear emitted RGB multiplier.
    pub radiance_micro: [u32; 3],
}
impl SceneCelestialDisc {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "celestial angular radius",
            u64::from(self.angular_radius_millidegrees),
            1,
            89_999,
        )?;
        require_range(
            "celestial rotation",
            u64::from(self.rotation_millidegrees),
            0,
            359_999,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneAtmosphere {
    pub horizon_radiance_micro: [u32; 3],
    pub zenith_radiance_micro: [u32; 3],
    pub gradient_exponent_micro: u32,
    /// Zero omits the solar disc. Direction and radiance come from the scene sun.
    pub sun_angular_radius_millidegrees: u32,
    pub fog: Option<SceneHeightFog>,
    /// Far-to-near layering order for celestial coverage.
    pub discs: Vec<SceneCelestialDisc>,
    /// Depth-tested celestial geometry composited after the background and discs.
    pub meshes: Vec<SceneCelestialMesh>,
    /// Finite atmospheric ray length used for background fog.
    pub background_distance_millimeters: u32,
}
impl SceneAtmosphere {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "sky gradient exponent",
            u64::from(self.gradient_exponent_micro),
            1,
            16_000_000,
        )?;
        require_range(
            "sky sun radius",
            u64::from(self.sun_angular_radius_millidegrees),
            0,
            89_999,
        )?;
        require_range(
            "sky background distance",
            u64::from(self.background_distance_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        require_range("celestial count", self.discs.len() as u64, 0, 64)?;
        for disc in &self.discs {
            disc.validate()?;
        }
        require_range("celestial mesh count", self.meshes.len() as u64, 0, 64)?;
        for mesh in &self.meshes {
            mesh.validate()?;
        }
        Ok(())
    }
    pub(super) fn validate_handles(
        &self,
        textures: u64,
        meshes: u64,
        materials: u64,
    ) -> Result<(), SceneValidationError> {
        for disc in &self.discs {
            if let Some(texture) = disc.texture {
                super::require_handle("celestial texture", texture.raw(), "texture", textures)?;
            }
        }
        for mesh in &self.meshes {
            super::require_handle("celestial mesh", mesh.mesh.raw(), "mesh", meshes)?;
            super::require_handle(
                "celestial material",
                mesh.material.raw(),
                "material",
                materials,
            )?;
        }
        Ok(())
    }
}

/// A finite mesh following the eye, rendered at far depth without shadow casting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCelestialMesh {
    pub direction: SceneDirection,
    pub angular_radius_millidegrees: u32,
    pub distance_millimeters: u32,
    /// Bounding radius of the source mesh before angular-size scaling.
    pub model_radius_millimeters: u32,
    pub rotation: super::SceneRotation,
    pub mesh: super::MeshHandle,
    pub material: super::MaterialHandle,
}
impl SceneCelestialMesh {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "celestial mesh angular radius",
            u64::from(self.angular_radius_millidegrees),
            1,
            89_999,
        )?;
        require_range(
            "celestial mesh distance",
            u64::from(self.distance_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        require_range(
            "celestial model radius",
            u64::from(self.model_radius_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        self.rotation.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_radius_and_orientation_fail_at_the_descriptor_boundary() {
        let mut disc = SceneCelestialDisc {
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            angular_radius_millidegrees: 500,
            rotation_millidegrees: 0,
            texture: None,
            radiance_micro: [4_000_000; 3],
        };
        assert!(disc.validate().is_ok());
        disc.angular_radius_millidegrees = 90_000;
        assert!(disc.validate().is_err());
        disc.angular_radius_millidegrees = 1;
        disc.rotation_millidegrees = 360_000;
        assert!(disc.validate().is_err());
    }
}
