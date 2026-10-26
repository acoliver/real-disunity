//! Script fields as JSON, with references resolved to plan entries.

use serde_json::{Map, Value as Json, json};

use crate::{
    Guid, ObjectId, Origin,
    package::{PackageAsset, PackageFont},
    scene::{Reference, Value},
};

use super::{PlanError, objects::Planner};

const GAME_OBJECT: i32 = 1;
const TRANSFORM: i32 = 4;
const RECT_TRANSFORM: i32 = 224;
/// The fileID of the texture itself in a texture asset, as opposed to one of its sprites.
const TEXTURE: i64 = 2_800_000;

/// Unity bookkeeping every serialized script carries; none of it is a script field.
const BOOKKEEPING: [&str; 11] = [
    "m_ObjectHideFlags",
    "m_CorrespondingSourceObject",
    "m_PrefabInstance",
    "m_PrefabAsset",
    "m_PrefabParentObject",
    "m_PrefabInternal",
    "m_GameObject",
    "m_Enabled",
    "m_EditorHideFlags",
    "m_Script",
    "m_EditorClassIdentifier",
];

impl Planner<'_> {
    /// The script fields of `owner`, without Unity's bookkeeping.
    pub(super) fn fields(&mut self, owner: ObjectId, fields: &Value) -> Result<Json, PlanError> {
        let mut out = Map::new();
        for (key, value) in fields.as_record().unwrap_or_default() {
            if key != "m_Name" && !BOOKKEEPING.contains(&key.as_str()) {
                out.insert(key.clone(), self.value(owner, key, value)?);
            }
        }
        Ok(Json::Object(out))
    }

    fn value(&mut self, owner: ObjectId, path: &str, value: &Value) -> Result<Json, PlanError> {
        Ok(match value {
            Value::Scalar(scalar) => Json::String(scalar.text().to_owned()),
            Value::Sequence(items) => Json::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| self.value(owner, &format!("{path}[{i}]"), item))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Record(fields) => {
                let mut out = Map::new();
                for (key, item) in fields {
                    out.insert(
                        key.clone(),
                        self.value(owner, &format!("{path}.{key}"), item)?,
                    );
                }
                Json::Object(out)
            }
            Value::Reference(reference) => self.reference(owner, path, reference)?,
        })
    }

    fn reference(
        &mut self,
        owner: ObjectId,
        path: &str,
        reference: &Reference,
    ) -> Result<Json, PlanError> {
        let scene = self.scene;
        let error = |message: String| scene.component_error(owner, format!("{path}: {message}"));
        match reference {
            Reference::Null => Ok(Json::Null),
            Reference::Object(target) => Ok(self.object_reference(*target).map_err(error)?),
            Reference::External { guid, file_id } => self.external(owner, path, *guid, *file_id),
            Reference::Package(PackageAsset::Font(font)) => {
                self.font(*font);
                Ok(json!({ "$font": font_name(*font) }))
            }
            Reference::Package(other) => Err(error(format!(
                "references package asset {other:?}, which scripts cannot use"
            ))
            .into()),
            Reference::Missing { .. } => {
                Err(error(format!("references a missing asset: {reference:?}")).into())
            }
        }
    }

    fn object_reference(&mut self, target: ObjectId) -> Result<Json, String> {
        let scene = self.scene;
        let object = scene.object(target).ok_or("references a removed object")?;
        if matches!(object.origin, Origin::Asset(_)) {
            return Ok(json!({ "$asset": self.asset(target) }));
        }
        match object.class_id {
            GAME_OBJECT => self
                .objects
                .get(&target)
                .map(|index| json!({ "$object": index }))
                .ok_or_else(|| "references an object outside the plan".to_owned()),
            TRANSFORM | RECT_TRANSFORM => self
                .owner_index(target)
                .map(|index| json!({ "$transform": index }))
                .map_err(|error| error.message),
            _ => self
                .components
                .get(&target)
                .map(|[object, component]| json!({ "$component": [object, component] }))
                .ok_or_else(|| format!("references a {} outside the plan", object.type_name)),
        }
    }

    /// A reference into a texture: one of its sprites or the texture itself.
    fn external(
        &mut self,
        owner: ObjectId,
        path: &str,
        guid: Guid,
        file_id: i64,
    ) -> Result<Json, PlanError> {
        let is_texture = self
            .project
            .asset(guid)
            .is_some_and(|asset| asset.importer == "TextureImporter");
        if !is_texture {
            let message =
                format!("{path}: references asset {guid} fileID {file_id}, which is not a texture");
            return Err(self.scene.component_error(owner, message).into());
        }
        if file_id == TEXTURE {
            return Ok(json!({ "$texture": self.texture(guid)? }));
        }
        Ok(json!({ "$sprite": self.sprite(owner, guid, file_id)? }))
    }
}

pub(super) fn font_name(font: PackageFont) -> &'static str {
    match font {
        PackageFont::Arial => "Arial",
    }
}

/// The sprite a renderer or tile references, as a sprite index.
pub(super) fn sprite_index(
    planner: &mut Planner<'_>,
    owner: ObjectId,
    reference: &Reference,
) -> Result<Option<usize>, PlanError> {
    match reference {
        Reference::Null => Ok(None),
        Reference::External { guid, file_id } => planner.sprite(owner, *guid, *file_id).map(Some),
        other => Err(planner
            .scene
            .component_error(owner, format!("references {other:?} as a sprite"))
            .into()),
    }
}
