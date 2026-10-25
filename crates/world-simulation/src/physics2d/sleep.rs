use crate::{ComponentDesc, ComponentError, ComponentHandle, World, WorldError};

impl World {
    pub fn body_sleeping(&self, handle: ComponentHandle) -> Result<bool, WorldError> {
        let ComponentDesc::Body2d(body) = &self.component(handle)?.desc else {
            return Err(WorldError::InvalidComponent(ComponentError::KindMismatch));
        };
        Ok(body.sleeping)
    }

    pub fn set_body_sleeping(
        &mut self,
        handle: ComponentHandle,
        sleeping: bool,
    ) -> Result<(), WorldError> {
        let ComponentDesc::Body2d(mut body) = self.component(handle)?.desc.clone() else {
            return Err(WorldError::InvalidComponent(ComponentError::KindMismatch));
        };
        body.sleeping = sleeping;
        if sleeping {
            body.velocity = glam::Vec2::ZERO;
            body.angular_velocity = 0.0;
        }
        self.set_component(handle, ComponentDesc::Body2d(body))?;
        if sleeping {
            self.physics.targets.remove(&handle);
        }
        Ok(())
    }
}
