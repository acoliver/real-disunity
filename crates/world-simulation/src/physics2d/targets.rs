use crate::{ComponentDesc, ComponentHandle, World, WorldError};
use glam::Vec2;

impl World {
    pub(super) fn body_velocity(
        &self,
        handle: ComponentHandle,
        body: &super::Body2d,
        start: Vec2,
        dt: f32,
    ) -> Vec2 {
        if body.sleeping {
            return Vec2::ZERO;
        }
        let velocity = if let Some(target) = self.physics.targets.get(&handle).filter(|_| dt > 0.0)
        {
            (*target - start) / dt
        } else if body.body_type == super::BodyType::Dynamic {
            body.velocity + self.physics.settings.gravity * body.gravity_scale * dt
        } else {
            body.velocity
        };
        velocity * body.linear_mask()
    }

    /// Requests motion through the next physics step, retaining collision resolution.
    pub fn move_body_position(
        &mut self,
        body: ComponentHandle,
        position: Vec2,
    ) -> Result<(), WorldError> {
        let component = self.component(body)?;
        if !position.is_finite() {
            return Err(super::PhysicsError::InvalidBody.into());
        }
        let ComponentDesc::Body2d(value) = &component.desc else {
            return Err(WorldError::InvalidComponent(
                crate::ComponentError::KindMismatch,
            ));
        };
        if value.body_type == super::BodyType::Static || !value.enabled {
            return Err(super::PhysicsError::InvalidBody.into());
        }
        self.set_body_sleeping(body, false)?;
        self.physics.targets.insert(body, position);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Body2d, BodyType};
    use crate::{ComponentDesc, ObjectDesc, World, WorldSettings};
    use glam::Vec2;

    #[test]
    fn target_moves_during_physics_and_rejects_nonfinite_input() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let body = world
            .add_component(
                object,
                ComponentDesc::Body2d(Body2d {
                    body_type: BodyType::Kinematic,
                    ..Default::default()
                }),
            )
            .unwrap();
        world.move_body_position(body, Vec2::new(1.0, 2.0)).unwrap();
        assert!(world.move_body_position(body, Vec2::NAN).is_err());
        assert_eq!(
            world.local_transform(object).unwrap().translation,
            glam::Vec3::ZERO
        );
        world.step_physics(0.02).unwrap();
        assert!(
            world
                .local_transform(object)
                .unwrap()
                .translation
                .truncate()
                .abs_diff_eq(Vec2::new(1.0, 2.0), 1e-5)
        );
        assert!(world.physics.targets.is_empty());
    }
}
