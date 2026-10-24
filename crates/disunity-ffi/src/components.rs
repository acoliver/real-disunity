use crate::ui::fields::{Canvas, CanvasScaler, RectTransform, Text};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Body {
    pub velocity: [f32; 2],
    pub body_type: BodyType,
    pub gravity_scale: f32,
    pub mass: f32,
    pub drag: f32,
    pub restitution: f32,
    pub angular_velocity: f32,
    pub angular_drag: f32,
    pub constraints: u8,
    pub material: Option<PhysicsMaterial>,
    pub simulated: bool,
    pub sleeping: bool,
    #[serde(skip)]
    pub move_position: Option<[f32; 2]>,
    #[serde(skip)]
    pub force: [f32; 2],
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    Dynamic,
    Kinematic,
    Static,
}
impl Default for Body {
    fn default() -> Self {
        Self {
            velocity: [0.0; 2],
            body_type: BodyType::Dynamic,
            gravity_scale: 0.0,
            mass: 1.0,
            drag: 0.0,
            restitution: 1.0,
            angular_velocity: 0.0,
            angular_drag: 0.0,
            constraints: 0,
            material: None,
            simulated: true,
            sleeping: false,
            move_position: None,
            force: [0.0; 2],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoxShape {
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub is_trigger: bool,
    pub material: Option<PhysicsMaterial>,
}
impl Default for BoxShape {
    fn default() -> Self {
        Self {
            size: [1.0; 2],
            offset: [0.0; 2],
            is_trigger: false,
            material: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Circle {
    pub radius: f32,
    pub offset: [f32; 2],
    pub is_trigger: bool,
    pub material: Option<PhysicsMaterial>,
}
impl Default for Circle {
    fn default() -> Self {
        Self {
            radius: 0.5,
            offset: [0.0; 2],
            is_trigger: false,
            material: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Polygon {
    pub points: Vec<[f32; 2]>,
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub is_trigger: bool,
    #[serde(default)]
    pub material: Option<PhysicsMaterial>,
}
impl Polygon {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.points.len() < 3
            || self.points.len() > 128
            || self
                .points
                .iter()
                .flatten()
                .chain(self.offset.iter())
                .any(|v| !v.is_finite())
        {
            return Err("polygon requires 3..128 finite convex vertices".into());
        }
        let mut sign = 0.0_f32;
        for i in 0..self.points.len() {
            let a = glam::Vec2::from(self.points[i]);
            let b = glam::Vec2::from(self.points[(i + 1) % self.points.len()]);
            let c = glam::Vec2::from(self.points[(i + 2) % self.points.len()]);
            let cross = (b - a).perp_dot(c - b);
            if cross.abs() < 1e-6 || (sign != 0.0 && sign.signum() != cross.signum()) {
                return Err("polygon must be strictly convex".into());
            }
            sign = cross;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sprite {
    pub sprite: Value,
    pub color: [f32; 4],
    pub size: [f32; 2],
    pub flip_x: bool,
    pub flip_y: bool,
    pub sorting_layer: i32,
    pub sorting_order: i32,
}
impl Default for Sprite {
    fn default() -> Self {
        Self {
            sprite: Value::Null,
            color: [1.0; 4],
            size: [1.0; 2],
            flip_x: false,
            flip_y: false,
            sorting_layer: 0,
            sorting_order: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Camera {
    pub orthographic: bool,
    pub orthographic_size: f32,
    pub background: [f32; 4],
    pub near: f32,
    pub far: f32,
    pub depth: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            orthographic: true,
            orthographic_size: 5.0,
            background: [0.0, 0.0, 0.0, 1.0],
            near: 0.1,
            far: 1000.0,
            depth: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Fields {
    Transform,
    Rect(RectTransform),
    Text(Text),
    Canvas(Canvas),
    CanvasScaler(CanvasScaler),
    Body(Body),
    Box(BoxShape),
    Circle(Circle),
    Polygon(Polygon),
    Tiles(crate::tiles::TilemapCollider),
    Sprite(Sprite),
    Camera(Camera),
    /// Script and Tilemap payloads are opaque and only ever replaced whole, never edited
    /// in place. Sharing them keeps the staged world copy taken for every mutating
    /// request from deep-copying large JSON trees.
    Metadata(std::sync::Arc<Value>),
}
#[derive(Clone, Debug)]
pub struct Component {
    pub handle: u64,
    pub object: u64,
    pub kind: String,
    pub enabled: bool,
    pub fields: Fields,
}
impl Component {
    pub fn is_transform(&self) -> bool {
        self.kind == "Transform" || self.kind == "RectTransform"
    }
    pub fn matches_kind(&self, k: &str) -> bool {
        self.kind == k || (k == "Transform" && self.is_transform())
    }
    fn snapshot_fields(&self) -> Value {
        let mut value = self.fields.value();
        if let Fields::Rect(r) = &self.fields {
            let (min, max) = r.offsets();
            value["offset_min"] = json!(min);
            value["offset_max"] = json!(max);
        }
        value
    }
    pub fn snapshot(&self) -> Value {
        let mut value = json!({"handle":self.handle,"object":self.object,"kind":self.kind,"enabled":self.enabled});
        value["fields"] = self.snapshot_fields();
        value
    }
}
impl Fields {
    pub fn value(&self) -> Value {
        match self {
            Self::Transform => json!({}),
            Self::Rect(v) => json!(v),
            Self::Text(v) => json!(v),
            Self::Canvas(v) => json!(v),
            Self::CanvasScaler(v) => json!(v),
            Self::Body(v) => json!(v),
            Self::Box(v) => json!(v),
            Self::Circle(v) => json!(v),
            Self::Polygon(v) => json!(v),
            Self::Tiles(v) => json!(v),
            Self::Sprite(v) => json!(v),
            Self::Camera(v) => json!(v),
            Self::Metadata(v) => Value::clone(v),
        }
    }
    pub fn parse(kind: &str, value: Value) -> Result<Self, String> {
        if !value.is_object() {
            return Err("component fields must be an object".into());
        }
        let result = match kind {
            "Transform" => {
                if value.as_object().unwrap().is_empty() {
                    Self::Transform
                } else {
                    return Err("use transform.set for Transform properties".into());
                }
            }
            "Rigidbody2D" => Self::Body(serde_json::from_value(value).map_err(|e| e.to_string())?),
            "BoxCollider2D" => Self::Box(serde_json::from_value(value).map_err(|e| e.to_string())?),
            "CircleCollider2D" => {
                Self::Circle(serde_json::from_value(value).map_err(|e| e.to_string())?)
            }
            "PolygonCollider2D" => {
                Self::Polygon(serde_json::from_value(value).map_err(|e| e.to_string())?)
            }
            "TilemapCollider2D" => Self::Tiles(crate::tiles::TilemapCollider::parse(value)?),
            "SpriteRenderer" => {
                Self::Sprite(serde_json::from_value(value).map_err(|e| e.to_string())?)
            }
            "Camera" => Self::Camera(serde_json::from_value(value).map_err(|e| e.to_string())?),
            "RectTransform" => Self::Rect(RectTransform::parse(value)?),
            "Text" => Self::Text(serde_json::from_value(value).map_err(|e| e.to_string())?),
            "Canvas" => Self::Canvas(serde_json::from_value(value).map_err(|e| e.to_string())?),
            "CanvasScaler" => {
                Self::CanvasScaler(serde_json::from_value(value).map_err(|e| e.to_string())?)
            }
            "Script" | "Tilemap" => Self::Metadata(std::sync::Arc::new(value)),
            _ => return Err(format!("unsupported component kind: {kind}")),
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Text(v) => v.validate()?,
            Self::Canvas(v) => v.validate()?,
            Self::CanvasScaler(v) => v.validate()?,
            Self::Body(b)
                if !b.velocity.iter().all(|v| v.is_finite())
                    || !b.mass.is_finite()
                    || b.mass <= 0.0
                    || !b.drag.is_finite()
                    || b.drag < 0.0
                    || !b.angular_velocity.is_finite()
                    || !b.angular_drag.is_finite()
                    || b.angular_drag < 0.0
                    || b.constraints > 7
                    || !b.gravity_scale.is_finite()
                    || !(0.0..=1.0).contains(&b.restitution) =>
            {
                return Err("invalid body values".into());
            }
            Self::Box(b)
                if b.size.iter().any(|v| !v.is_finite() || *v <= 0.0)
                    || b.offset.iter().any(|v| !v.is_finite()) =>
            {
                return Err("invalid box geometry".into());
            }
            Self::Circle(c)
                if !c.radius.is_finite()
                    || c.radius <= 0.0
                    || c.offset.iter().any(|v| !v.is_finite()) =>
            {
                return Err("invalid circle geometry".into());
            }
            Self::Polygon(p) => p.validate()?,
            Self::Camera(c) if c.orthographic_size <= 0.0 || c.near < 0.0 || c.far <= c.near => {
                return Err("invalid camera range".into());
            }
            _ => {}
        }
        self.material().map_or(Ok(()), PhysicsMaterial::validate)
    }
    pub(crate) fn material(&self) -> Option<&PhysicsMaterial> {
        match self {
            Self::Body(v) => v.material.as_ref(),
            Self::Box(v) => v.material.as_ref(),
            Self::Circle(v) => v.material.as_ref(),
            Self::Polygon(v) => v.material.as_ref(),
            Self::Tiles(v) => v.material.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsMaterial {
    pub bounciness: f32,
    pub friction: f32,
    pub asset: Value,
}
impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            bounciness: 0.0,
            friction: 0.4,
            asset: Value::Null,
        }
    }
}
impl PhysicsMaterial {
    fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.bounciness)
            || !self.friction.is_finite()
            || self.friction < 0.0
        {
            return Err("invalid physics material".into());
        }
        Ok(())
    }
}
