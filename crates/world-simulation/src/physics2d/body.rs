use super::PhysicsError;
use glam::{Quat, Vec2, Vec3};

/// Simulation coordinates in an explicitly selected native plane, in metres and radians.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicsPlane {
    #[default]
    Xy,
    Xz,
}
impl PhysicsPlane {
    pub fn project(self, p: Vec3) -> Vec2 {
        match self {
            Self::Xy => p.truncate(),
            Self::Xz => Vec2::new(p.x, p.z),
        }
    }
    pub fn vector(self, p: Vec2) -> Vec3 {
        match self {
            Self::Xy => p.extend(0.0),
            Self::Xz => Vec3::new(p.x, 0.0, p.y),
        }
    }
    pub fn rotation(self, angle: f32) -> Quat {
        match self {
            Self::Xy => Quat::from_rotation_z(angle),
            Self::Xz => Quat::from_rotation_y(-angle),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyType {
    Static,
    Kinematic,
    #[default]
    Dynamic,
}

/// Unity's `RigidbodyInterpolation2D`, without extrapolation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// Drawn at the pose of the last physics step.
    #[default]
    None,
    /// Drawn between its poses before and after the last physics step. Physics and scripts
    /// still see the step pose.
    Interpolate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body2d {
    pub body_type: BodyType,
    pub enabled: bool,
    pub shared_material: Option<super::PhysicsMaterialHandle>,
    pub sleeping: bool,
    pub mass: f32,
    pub velocity: Vec2,
    /// Radians per second.
    pub angular_velocity: f32,
    pub gravity_scale: f32,
    pub freeze_rotation: bool,
    /// Position axes excluded from motion and impulses: X = 1, Y = 2.
    pub freeze_position: u8,
    pub restitution: f32,
    pub interpolation: Interpolation,
}
impl Default for Body2d {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            enabled: true,
            shared_material: None,
            sleeping: false,
            mass: 1.0,
            velocity: Vec2::ZERO,
            angular_velocity: 0.0,
            gravity_scale: 1.0,
            freeze_rotation: false,
            freeze_position: 0,
            restitution: 0.0,
            interpolation: Interpolation::None,
        }
    }
}
impl Body2d {
    pub(crate) fn linear_mask(&self) -> Vec2 {
        Vec2::new(
            f32::from(self.freeze_position & 1 == 0),
            f32::from(self.freeze_position & 2 == 0),
        )
    }

    pub(crate) fn validate(&self) -> Result<(), PhysicsError> {
        if self.freeze_position > 3
            || !self.mass.is_finite()
            || self.mass <= 0.0
            || !self.velocity.is_finite()
            || !self.angular_velocity.is_finite()
            || !self.gravity_scale.is_finite()
            || !self.restitution.is_finite()
            || !(0.0..=1.0).contains(&self.restitution)
        {
            return Err(PhysicsError::InvalidBody);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsSettings {
    pub plane: PhysicsPlane,
    pub gravity: Vec2,
    /// Material restitution applies only above this closing speed, in metres per second.
    pub bounce_threshold: f32,
    pub max_iterations: u32,
}
impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            plane: PhysicsPlane::Xy,
            gravity: Vec2::new(0.0, -9.81),
            bounce_threshold: 1.0,
            max_iterations: 2048,
        }
    }
}
impl PhysicsSettings {
    pub(crate) fn validate(self) -> Result<(), PhysicsError> {
        if !self.gravity.is_finite()
            || !self.bounce_threshold.is_finite()
            || self.bounce_threshold < 0.0
            || self.max_iterations == 0
            || self.max_iterations > 65536
        {
            return Err(PhysicsError::InvalidSettings);
        }
        Ok(())
    }
}
