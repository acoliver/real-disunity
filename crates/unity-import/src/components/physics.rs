use super::{ComponentError, PHYSICS_MATERIAL_2D, RIGIDBODY_2D};
use crate::scene::{
    ObjectId, Reference, Scene, Value,
    read::{as_integer, number, object_id},
};

/// `PhysicsMaterialCombine2D`: how two touching colliders' values combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Combine {
    Average,
    /// Geometric mean, `sqrt(a * b)`.
    Mean,
    Multiply,
    Minimum,
    Maximum,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct PhysicsMaterial {
    pub friction: f64,
    pub bounciness: f64,
    pub friction_combine: Combine,
    pub bounce_combine: Combine,
}

impl PhysicsMaterial {
    /// The material Unity uses for a collider when neither it nor its rigidbody names one.
    pub const DEFAULT: Self = Self {
        friction: 0.4,
        bounciness: 0.0,
        friction_combine: Combine::Mean,
        bounce_combine: Combine::Maximum,
    };
}

impl Scene {
    pub fn physics_material(&self, id: ObjectId) -> Result<PhysicsMaterial, ComponentError> {
        let fields = self.expect_class(id, PHYSICS_MATERIAL_2D, "PhysicsMaterial2D")?;
        let error = |message: &str| self.component_error(id, message);
        let default = PhysicsMaterial::DEFAULT;
        // Unity 2017 materials predate the combine settings and behave as the defaults.
        Ok(PhysicsMaterial {
            friction: number(fields, "friction", default.friction)
                .ok_or_else(|| error("invalid friction"))?,
            bounciness: number(fields, "bounciness", default.bounciness)
                .ok_or_else(|| error("invalid bounciness"))?,
            friction_combine: combine(fields, "m_FrictionCombine", default.friction_combine)
                .ok_or_else(|| error("invalid m_FrictionCombine"))?,
            bounce_combine: combine(fields, "m_BounceCombine", default.bounce_combine)
                .ok_or_else(|| error("invalid m_BounceCombine"))?,
        })
    }

    /// The material a 2D collider uses: its own, else its attached rigidbody's, else the default.
    /// The attached rigidbody is the nearest `Rigidbody2D` on the collider's object or a parent.
    pub fn collider_material(&self, collider: ObjectId) -> Result<PhysicsMaterial, ComponentError> {
        let fields = &self
            .object(collider)
            .ok_or_else(|| self.component_error(collider, "is not in the scene"))?
            .fields;
        if let Some(material) = self.material_field(collider, fields)? {
            return Ok(material);
        }
        let body = fields
            .get("m_GameObject")
            .and_then(object_id)
            .and_then(|owner| self.attached_rigidbody(owner))
            .and_then(|body| Some((body, &self.object(body)?.fields)));
        match body {
            Some((body, body_fields)) => Ok(self
                .material_field(body, body_fields)?
                .unwrap_or(PhysicsMaterial::DEFAULT)),
            None => Ok(PhysicsMaterial::DEFAULT),
        }
    }

    fn material_field(
        &self,
        owner: ObjectId,
        fields: &Value,
    ) -> Result<Option<PhysicsMaterial>, ComponentError> {
        match fields.get("m_Material").and_then(Value::as_reference) {
            None | Some(Reference::Null) => Ok(None),
            Some(Reference::Object(material)) => self.physics_material(material).map(Some),
            Some(other) => Err(self.component_error(
                owner,
                format!("physics material {other:?} is not in the project"),
            )),
        }
    }

    fn attached_rigidbody(&self, game_object: ObjectId) -> Option<ObjectId> {
        let mut current = Some(game_object);
        while let Some(object) = current {
            let view = self.game_object(object)?;
            let body = view
                .components
                .iter()
                .copied()
                .find(|id| self.object(*id).is_some_and(|o| o.class_id == RIGIDBODY_2D));
            if body.is_some() {
                return body;
            }
            current = self
                .transform_of(object)
                .and_then(|transform| self.transform(transform)?.parent)
                .and_then(|parent| self.transform(parent)?.game_object);
        }
        None
    }
}

fn combine(fields: &Value, key: &str, default: Combine) -> Option<Combine> {
    let Some(value) = fields.get(key) else {
        return Some(default);
    };
    Some(match as_integer(value)? {
        0 => Combine::Average,
        1 => Combine::Mean,
        2 => Combine::Multiply,
        3 => Combine::Minimum,
        4 => Combine::Maximum,
        _ => return None,
    })
}
