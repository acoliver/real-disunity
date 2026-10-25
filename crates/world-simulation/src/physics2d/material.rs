use super::PhysicsError;
use crate::{ComponentDesc, ComponentHandle, LimitKind, World, WorldError};

/// Shared material identity scoped to its owning world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicsMaterialHandle {
    world: u64,
    index: usize,
}

/// Validated surface coefficients. Friction may exceed one; restitution cannot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsMaterial2d {
    friction: f32,
    bounciness: f32,
}
impl PhysicsMaterial2d {
    pub fn new(friction: f32, bounciness: f32) -> Result<Self, PhysicsError> {
        if !friction.is_finite()
            || friction < 0.0
            || !bounciness.is_finite()
            || !(0.0..=1.0).contains(&bounciness)
        {
            return Err(PhysicsError::InvalidMaterial);
        }
        Ok(Self {
            friction,
            bounciness,
        })
    }
    pub const fn friction(self) -> f32 {
        self.friction
    }
    pub const fn bounciness(self) -> f32 {
        self.bounciness
    }
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        Self {
            friction: self.friction.sqrt() * other.friction.sqrt(),
            bounciness: self.bounciness.max(other.bounciness),
        }
    }
}
impl World {
    pub fn create_physics_material(
        &mut self,
        value: PhysicsMaterial2d,
    ) -> Result<PhysicsMaterialHandle, WorldError> {
        self.ensure_allocation_allowed()?;
        if self.physics.materials.len() >= self.assets_limit as usize {
            return Err(WorldError::CapacityExceeded(LimitKind::Objects));
        }
        let handle = PhysicsMaterialHandle {
            world: self.identity,
            index: self.physics.materials.len(),
        };
        self.physics.materials.push(value);
        Ok(handle)
    }
    pub fn physics_material(
        &self,
        handle: PhysicsMaterialHandle,
    ) -> Result<PhysicsMaterial2d, WorldError> {
        if handle.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        self.physics
            .materials
            .get(handle.index)
            .copied()
            .ok_or(PhysicsError::InvalidMaterial.into())
    }
    pub fn set_physics_material(
        &mut self,
        handle: PhysicsMaterialHandle,
        value: PhysicsMaterial2d,
    ) -> Result<(), WorldError> {
        self.physics_material(handle)?;
        self.physics.materials[handle.index] = value;
        Ok(())
    }
    /// Collider asset, then nearest attached body's asset, then the existing body restitution.
    pub fn collider_material(
        &self,
        handle: ComponentHandle,
    ) -> Result<PhysicsMaterial2d, WorldError> {
        let c = self.component(handle)?;
        let ComponentDesc::Collider2d(desc) = &c.desc else {
            return Err(PhysicsError::InvalidMaterial.into());
        };
        let body = self.attached_body(c.object)?;
        let (body_material, restitution) = match body.map(|c| &c.desc) {
            Some(ComponentDesc::Body2d(b)) => (b.shared_material, b.restitution),
            _ => (None, 0.0),
        };
        desc.shared_material.or(body_material).map_or_else(
            || PhysicsMaterial2d::new(0.0, restitution).map_err(WorldError::from),
            |h| self.physics_material(h),
        )
    }
    pub(crate) fn validate_physics_material(&self, desc: &ComponentDesc) -> Result<(), WorldError> {
        let handle = match desc {
            ComponentDesc::Body2d(b) => b.shared_material,
            ComponentDesc::Collider2d(c) => c.shared_material,
            _ => None,
        };
        if let Some(handle) = handle {
            self.physics_material(handle)?;
        }
        Ok(())
    }
}
