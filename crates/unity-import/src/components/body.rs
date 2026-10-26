use super::{ComponentError, PHYSICS_MATERIAL_2D, RIGIDBODY_2D};
use crate::scene::{
    ObjectId, Reference, Scene, Value,
    read::{XYZW, flag, integer, number, vector},
};

pub(crate) const BOX_COLLIDER_2D: i32 = 61;
pub(crate) const CIRCLE_COLLIDER_2D: i32 = 58;
pub(crate) const TILEMAP_COLLIDER_2D: i32 = 19_719_996;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum BodyType {
    Dynamic,
    Kinematic,
    Static,
}

/// `RigidbodyInterpolation2D`: how the drawn pose follows the fixed-step pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Interpolation {
    None,
    Interpolate,
    Extrapolate,
}

/// An undamped `Rigidbody2D` with explicit mass. The engine detects collisions continuously
/// for every body, so the collision detection mode is not part of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigidbody2D {
    pub body_type: BodyType,
    pub simulated: bool,
    pub mass: f64,
    pub gravity_scale: f64,
    /// `RigidbodyConstraints2D` bits: freeze position X 1, Y 2, rotation 4.
    pub constraints: u8,
    pub interpolation: Interpolation,
    /// The body's own `PhysicsMaterial2D` object, if any.
    pub material: Option<ObjectId>,
}

/// Settings every 2D collider shares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider2D {
    pub enabled: bool,
    pub trigger: bool,
    pub offset: [f64; 2],
    /// The collider's own `PhysicsMaterial2D` object; without one the attached body's applies.
    pub material: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub enum ColliderShape {
    Box {
        size: [f64; 2],
    },
    Circle {
        radius: f64,
    },
    /// `TilemapCollider2D`: shapes from the Tilemap on the same object.
    Tilemap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub settings: Collider2D,
    pub shape: ColliderShape,
}

impl Scene {
    pub fn rigidbody_2d(&self, id: ObjectId) -> Result<Rigidbody2D, ComponentError> {
        let fields = self.expect_class(id, RIGIDBODY_2D, "Rigidbody2D")?;
        let error = |message: &str| self.component_error(id, message);
        check_body_supported(fields).map_err(error)?;
        let body_type = match integer(fields, "m_BodyType", 0) {
            Some(0) => BodyType::Dynamic,
            Some(1) => BodyType::Kinematic,
            Some(2) => BodyType::Static,
            _ => return Err(error("invalid m_BodyType")),
        };
        let interpolation = match integer(fields, "m_Interpolate", 0) {
            Some(0) => Interpolation::None,
            Some(1) => Interpolation::Interpolate,
            Some(2) => Interpolation::Extrapolate,
            _ => return Err(error("invalid m_Interpolate")),
        };
        let constraints = integer(fields, "m_Constraints", 0)
            .and_then(|bits| u8::try_from(bits).ok())
            .filter(|bits| *bits <= 7)
            .ok_or_else(|| error("invalid m_Constraints"))?;
        Ok(Rigidbody2D {
            body_type,
            simulated: self.field(id, "m_Simulated", flag(fields, "m_Simulated", true))?,
            mass: self.field(id, "m_Mass", number(fields, "m_Mass", 1.0))?,
            gravity_scale: self.field(
                id,
                "m_GravityScale",
                number(fields, "m_GravityScale", 1.0),
            )?,
            constraints,
            interpolation,
            material: self.material_object(id, fields)?,
        })
    }

    /// A `BoxCollider2D`, `CircleCollider2D` or `TilemapCollider2D`.
    pub fn collider(&self, id: ObjectId) -> Result<Collider, ComponentError> {
        let object = self
            .object(id)
            .ok_or_else(|| self.component_error(id, "is not in the scene"))?;
        let fields = &object.fields;
        let field = |key, default| self.field(id, key, number(fields, key, default));
        let shape = match object.class_id {
            BOX_COLLIDER_2D => ColliderShape::Box {
                size: self.field(id, "m_Size", vector(fields, "m_Size", XYZW, [1.0, 1.0]))?,
            },
            CIRCLE_COLLIDER_2D => ColliderShape::Circle {
                radius: field("m_Radius", 0.5)?,
            },
            TILEMAP_COLLIDER_2D => {
                if field("m_ExtrusionFactor", 0.0)? != 0.0
                    || flag(fields, "m_UseDelaunayMesh", false) != Some(false)
                {
                    return Err(
                        self.component_error(id, "tilemap collider extrusion is not supported")
                    );
                }
                ColliderShape::Tilemap
            }
            _ => return Err(self.component_error(id, "is not a supported 2D collider")),
        };
        check_collider_supported(fields).map_err(|message| self.component_error(id, message))?;
        if field("m_EdgeRadius", 0.0)? != 0.0 {
            return Err(self.component_error(id, "collider edge radius is not supported"));
        }
        let settings = Collider2D {
            enabled: self.field(id, "m_Enabled", flag(fields, "m_Enabled", true))?,
            trigger: self.field(id, "m_IsTrigger", flag(fields, "m_IsTrigger", false))?,
            offset: self.field(id, "m_Offset", vector(fields, "m_Offset", XYZW, [0.0; 2]))?,
            material: self.material_object(id, fields)?,
        };
        Ok(Collider { settings, shape })
    }

    fn material_object(
        &self,
        owner: ObjectId,
        fields: &Value,
    ) -> Result<Option<ObjectId>, ComponentError> {
        match fields.get("m_Material").and_then(Value::as_reference) {
            None | Some(Reference::Null) => Ok(None),
            Some(Reference::Object(material)) => {
                self.expect_class(material, PHYSICS_MATERIAL_2D, "PhysicsMaterial2D")?;
                Ok(Some(material))
            }
            Some(other) => Err(self.component_error(
                owner,
                format!("physics material {other:?} is not in the project"),
            )),
        }
    }
}

fn check_body_supported(fields: &Value) -> Result<(), &'static str> {
    // Unity 2017 names the damping fields drag.
    for key in [
        "m_LinearDamping",
        "m_AngularDamping",
        "m_LinearDrag",
        "m_AngularDrag",
    ] {
        if number(fields, key, 0.0) != Some(0.0) {
            return Err("rigidbody damping is not supported");
        }
    }
    if flag(fields, "m_UseAutoMass", false) != Some(false) {
        return Err("automatic rigidbody mass is not supported");
    }
    if flag(fields, "m_UseFullKinematicContacts", false) != Some(false) {
        return Err("full kinematic contacts are not supported");
    }
    if integer(fields, "m_SleepingMode", 1) == Some(2) {
        return Err("bodies that start asleep are not supported");
    }
    if layer_bits(fields, "m_IncludeLayers", 0) != Some(0)
        || layer_bits(fields, "m_ExcludeLayers", 0) != Some(0)
    {
        return Err("rigidbody layer overrides are not supported");
    }
    Ok(())
}

fn check_collider_supported(fields: &Value) -> Result<(), &'static str> {
    for key in ["m_UsedByEffector", "m_UsedByComposite", "m_AutoTiling"] {
        if flag(fields, key, false) != Some(false) {
            return Err("effectors, composites and auto tiling are not supported");
        }
    }
    if integer(fields, "m_CompositeOperation", 0) != Some(0) {
        return Err("composite collider operations are not supported");
    }
    if layer_bits(fields, "m_IncludeLayers", 0) != Some(0)
        || layer_bits(fields, "m_ExcludeLayers", 0) != Some(0)
    {
        return Err("collider layer overrides are not supported");
    }
    let every = i64::from(u32::MAX);
    for key in [
        "m_ForceSendLayers",
        "m_ForceReceiveLayers",
        "m_ContactCaptureLayers",
        "m_CallbackLayers",
    ] {
        if layer_bits(fields, key, every) != Some(every) {
            return Err("collider contact layer filters are not supported");
        }
    }
    Ok(())
}

/// A serialized `LayerMask`'s bits, or `default` when the field is absent.
fn layer_bits(fields: &Value, key: &str, default: i64) -> Option<i64> {
    fields
        .get(key)
        .map_or(Some(default), |mask| integer(mask, "m_Bits", default))
}
