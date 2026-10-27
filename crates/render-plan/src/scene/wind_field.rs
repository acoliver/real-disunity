use super::{SceneValidationError, TextureHandle};

/// Planar, static-slice approximation of a spatial wind field. Linear RG values
/// encode signed scene XY force as `2 * value - 1`. Sampling clamps at the domain
/// edge. Up to two static modifiers are accumulated. This type has no vertical or temporal texture interpolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePlanarWindField {
    texture: TextureHandle,
    origin_millimeters: [i32; 2],
    extent_millimeters: [u32; 2],
    modifiers: [Option<SceneStaticWindModifier>; 2],
}
impl ScenePlanarWindField {
    pub fn new(
        texture: TextureHandle,
        origin_millimeters: [i32; 2],
        extent_millimeters: [u32; 2],
        modifiers: [Option<SceneStaticWindModifier>; 2],
    ) -> Result<Self, SceneValidationError> {
        if extent_millimeters.contains(&0) {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "wind field extent",
            });
        }
        Ok(Self {
            texture,
            origin_millimeters,
            extent_millimeters,
            modifiers,
        })
    }
    pub const fn texture(self) -> TextureHandle {
        self.texture
    }
    pub const fn origin_millimeters(self) -> [i32; 2] {
        self.origin_millimeters
    }
    pub const fn extent_millimeters(self) -> [u32; 2] {
        self.extent_millimeters
    }
    pub const fn modifiers(self) -> [Option<SceneStaticWindModifier>; 2] {
        self.modifiers
    }
}

/// Static capsule force-volume approximation. It omits harmonic damping,
/// object-origin response, size attenuation and category masks. Strength is
/// signed; zero strength contributes exactly zero. Capsule distance uses the
/// scene Z axis, with cubic smoothstep attenuation between the two radii.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneStaticWindModifier {
    center: [i32; 3],
    half_height: u32,
    radii: [u32; 2],
    strength: i32,
}
impl SceneStaticWindModifier {
    pub fn new(
        center: [i32; 3],
        half_height: u32,
        radii: [u32; 2],
        strength: i32,
    ) -> Result<Self, SceneValidationError> {
        if radii[1] <= radii[0] {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "wind modifier falloff",
            });
        }
        Ok(Self {
            center,
            half_height,
            radii,
            strength,
        })
    }
    pub const fn center_millimeters(self) -> [i32; 3] {
        self.center
    }
    pub const fn half_height_millimeters(self) -> u32 {
        self.half_height
    }
    pub const fn radii_millimeters(self) -> [u32; 2] {
        self.radii
    }
    pub const fn strength_millimeters(self) -> i32 {
        self.strength
    }
}
