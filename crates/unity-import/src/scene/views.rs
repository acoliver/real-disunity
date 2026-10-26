use super::{
    ObjectId, Reference, Scene, Value,
    read::{XYZW, object_id, text, vector},
};
use crate::yaml::Scalar;

pub(crate) const GAME_OBJECT: i32 = 1;
pub(crate) const TRANSFORM: i32 = 4;
pub(crate) const RECT_TRANSFORM: i32 = 224;

#[derive(Clone, Debug, PartialEq)]
pub struct GameObject {
    pub name: String,
    pub tag: String,
    pub layer: i64,
    pub active: bool,
    /// Components in authored order, followed by components a prefab instance added.
    pub components: Vec<ObjectId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub game_object: Option<ObjectId>,
    pub parent: Option<ObjectId>,
    pub children: Vec<ObjectId>,
    pub local_position: [f64; 3],
    pub local_rotation: [f64; 4],
    pub local_scale: [f64; 3],
}

impl Scene {
    /// The `GameObject` view of `id`, or `None` when it is not a well-formed `GameObject`.
    pub fn game_object(&self, id: ObjectId) -> Option<GameObject> {
        let object = self.object(id).filter(|o| o.class_id == GAME_OBJECT)?;
        let fields = &object.fields;
        let listed = fields
            .get("m_Component")
            .and_then(Value::as_sequence)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| object_id(entry.get("component")?));
        Some(GameObject {
            name: text(fields, "m_Name")?,
            tag: text(fields, "m_TagString").unwrap_or_else(|| "Untagged".into()),
            layer: fields
                .get("m_Layer")
                .map_or(Some(0), |value| value.as_scalar()?.as_i64())?,
            active: fields
                .get("m_IsActive")
                .and_then(Value::as_scalar)
                .map_or(Some(true), Scalar::as_bool)?,
            components: self.linked(listed, id, "m_GameObject", |_| true),
        })
    }

    /// The `Transform` or `RectTransform` view of `id`.
    pub fn transform(&self, id: ObjectId) -> Option<Transform> {
        let object = self
            .object(id)
            .filter(|o| matches!(o.class_id, TRANSFORM | RECT_TRANSFORM))?;
        let fields = &object.fields;
        let parent = fields.get("m_Father").and_then(object_id);
        let listed = fields
            .get("m_Children")
            .and_then(Value::as_sequence)
            .unwrap_or_default()
            .iter()
            .filter_map(object_id);
        let is_transform = |id: ObjectId| {
            self.object(id)
                .is_some_and(|o| matches!(o.class_id, TRANSFORM | RECT_TRANSFORM))
        };
        Some(Transform {
            game_object: fields.get("m_GameObject").and_then(object_id),
            parent: parent.filter(|parent| self.object(*parent).is_some()),
            children: self.linked(listed, id, "m_Father", is_transform),
            local_position: vector(fields, "m_LocalPosition", XYZW, [0.0; 3])?,
            local_rotation: vector(fields, "m_LocalRotation", XYZW, [0.0, 0.0, 0.0, 1.0])?,
            local_scale: vector(fields, "m_LocalScale", XYZW, [1.0; 3])?,
        })
    }

    /// The first `Transform` or `RectTransform` component of a `GameObject`.
    pub fn transform_of(&self, game_object: ObjectId) -> Option<ObjectId> {
        self.game_object(game_object)?
            .components
            .into_iter()
            .find(|id| self.transform(*id).is_some())
    }

    /// The `m_Script` reference of a `MonoBehaviour`.
    pub fn script(&self, id: ObjectId) -> Option<Reference> {
        self.object(id)?.fields.get("m_Script")?.as_reference()
    }

    /// Objects listed by `owner` whose `link` field points back at it, then unlisted objects
    /// whose `link` points at `owner`. Removed objects are skipped.
    fn linked(
        &self,
        listed: impl Iterator<Item = ObjectId>,
        owner: ObjectId,
        link: &str,
        accept: impl Fn(ObjectId) -> bool,
    ) -> Vec<ObjectId> {
        let points_here = |id: ObjectId| {
            self.object(id)
                .and_then(|object| object.fields.get(link))
                .and_then(object_id)
                == Some(owner)
        };
        let mut ids: Vec<_> = listed
            .filter(|id| points_here(*id) && accept(*id))
            .collect();
        let extra: Vec<_> = self
            .objects()
            .map(|(id, _)| id)
            .filter(|id| !ids.contains(id) && points_here(*id) && accept(*id))
            .collect();
        ids.extend(extra);
        ids
    }
}
