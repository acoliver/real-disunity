use crate::{
    DisunityWorld,
    components::{Body, BodyType, Fields},
};
use glam::Vec2;
use serde_json::Value;
impl DisunityWorld {
    pub(crate) fn attached_body(&self, object: u64) -> Option<&crate::components::Component> {
        let mut current = Some(object);
        while let Some(h) = current {
            if let Some(c) = self
                .components
                .iter()
                .flatten()
                .find(|c| c.object == h && matches!(c.fields, Fields::Body(_)))
            {
                return Some(c);
            }
            current = self.metadata.get(&h)?.parent;
        }
        None
    }
    pub(crate) fn wake_transform(&mut self, object: u64) -> Result<(), String> {
        self.object_index(object)?;
        let owner = self.attached_body(object).map(|c| c.handle);
        let mut affected = vec![object];
        let mut i = 0;
        while i < affected.len() {
            affected.extend(self.children(affected[i]));
            i += 1;
        }
        for c in self.components.iter_mut().flatten() {
            if (affected.contains(&c.object) || owner == Some(c.handle))
                && let Fields::Body(b) = &mut c.fields
            {
                b.sleeping = false;
            }
        }
        Ok(())
    }
    fn body_mut(&mut self, component: u64) -> Result<&mut Body, String> {
        let i = self.component_index(component)?;
        match &mut self.components[i].as_mut().unwrap().fields {
            Fields::Body(body) => Ok(body),
            _ => Err("operation requires Rigidbody2D".into()),
        }
    }
    pub(crate) fn move_body(
        &mut self,
        component: u64,
        position: [f32; 2],
    ) -> Result<Value, String> {
        if position.iter().any(|v| !v.is_finite()) {
            return Err("body position must be finite".into());
        }
        let body = self.body_mut(component)?;
        if body.body_type == BodyType::Static {
            return Err("static bodies cannot MovePosition".into());
        }
        body.move_position = Some(position);
        body.sleeping = false;
        Ok(Value::Null)
    }
    pub(crate) fn sleep_body(&mut self, component: u64, sleep: bool) -> Result<Value, String> {
        let body = self.body_mut(component)?;
        body.sleeping = sleep;
        if sleep {
            body.velocity = [0.0; 2];
            body.angular_velocity = 0.0;
            body.force = [0.0; 2];
            body.move_position = None;
        }
        Ok(Value::Null)
    }
    pub(crate) fn force_body(
        &mut self,
        component: u64,
        force: [f32; 2],
        mode: &str,
    ) -> Result<Value, String> {
        if force.iter().any(|v| !v.is_finite()) {
            return Err("force must be finite".into());
        }
        if mode != "force" && mode != "impulse" {
            return Err("force mode must be force or impulse".into());
        }
        let body = self.body_mut(component)?;
        if body.body_type != BodyType::Dynamic {
            return Err("force requires a dynamic body".into());
        }
        if mode == "impulse" {
            body.velocity = (Vec2::from(body.velocity) + Vec2::from(force) / body.mass).to_array();
        } else {
            body.force = (Vec2::from(body.force) + Vec2::from(force)).to_array();
        }
        body.sleeping = false;
        Ok(Value::Null)
    }
}
