use crate::{
    catalog::Project,
    model::{AssetKind, Diagnostic, ImportProfile, ImportSidecar, SourceRef},
    prefab::{Expansion, Record, diagnostic},
};
use serde_json::{Value, json};

pub(crate) struct Resolver<'a> {
    pub project: &'a Project<'a>,
    pub expansion: &'a Expansion,
    pub profile: &'a ImportProfile,
    pub sidecar: &'a ImportSidecar,
    pub diagnostics: Vec<Diagnostic>,
}
impl Resolver<'_> {
    pub fn fields(&mut self, record: &Record) -> Value {
        let mut value = record.doc.fields.clone();
        if let Some(map) = value.as_object_mut() {
            for (field, value) in map {
                // Correspondence/override targets describe provenance, not live object links.
                if matches!(
                    field.as_str(),
                    "m_CorrespondingSourceObject"
                        | "m_PrefabParentObject"
                        | "m_PrefabInstance"
                        | "m_PrefabInternal"
                        | "m_PrefabAsset"
                        | "m_SourcePrefab"
                        | "m_ParentPrefab"
                        | "m_Modification"
                ) {
                    continue;
                }
                *value = self.resolve(value, record, field, 0);
            }
        }
        value
    }
    fn resolve(&mut self, value: &Value, record: &Record, field: &str, depth: usize) -> Value {
        if depth > 96 {
            self.diagnostics.push(diagnostic(
                record,
                "error",
                "reference_depth_limit",
                Some(field),
                "reference or value recursion exceeds 96",
            ));
            return json!({"$missing":value});
        }
        if value.get("fileID").is_some() && value["fileID"].as_i64().is_none() {
            self.diagnostics.push(diagnostic(
                record,
                "error",
                "invalid_reference_id",
                Some(field),
                "fileID must be a signed 64-bit integer",
            ));
            return json!({"$missing":value});
        }
        if let Some(id) = value.get("fileID").and_then(Value::as_i64) {
            return self.pointer(value, record, field, depth, id);
        }
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            self.resolve(v, record, &format!("{field}.{k}"), depth + 1),
                        )
                    })
                    .collect(),
            ),
            Value::Array(array) => Value::Array(
                array
                    .iter()
                    .enumerate()
                    .map(|(i, v)| self.resolve(v, record, &format!("{field}[{i}]"), depth + 1))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    fn pointer(
        &mut self,
        value: &Value,
        record: &Record,
        field: &str,
        depth: usize,
        id: i64,
    ) -> Value {
        if id == 0 {
            return Value::Null;
        }
        if self.invalid_guid(value, record, field) {
            return json!({"$missing":value});
        }
        let guid = value["guid"].as_str().unwrap_or(&record.source.guid);
        let patch = self
            .sidecar
            .reference_substitutions
            .iter()
            .find(|p| p.guid == guid && p.file_id.is_none_or(|f| f == id))
            .cloned();
        if let Some(patch) = patch {
            if patch.reason.trim().is_empty() {
                self.diagnostics.push(diagnostic(
                    record,
                    "error",
                    "sidecar_reason_missing",
                    Some(field),
                    "reference substitution requires reason",
                ));
            } else {
                self.diagnostics.push(diagnostic(
                    record,
                    "info",
                    "sidecar_reference_substitution",
                    Some(field),
                    &patch.reason,
                ));
                self.source_defect(value, record, field, id);
                let mut replacement = self.resolve(&patch.replacement, record, field, depth + 1);
                if replacement.get("$builtin").is_some() {
                    replacement["guid"] = json!(guid);
                    replacement["file_id"] = json!(id);
                }
                return replacement;
            }
        }
        if let Some(target) = self.lookup(value, record, field, id) {
            return target;
        }
        let code = if guid.starts_with("0000000000000000") {
            "builtin_replacement_required"
        } else if self.project.assets.contains_key(guid) {
            "missing_file_id"
        } else {
            "missing_guid"
        };
        self.diagnostics.push(diagnostic(
            record,
            "error",
            code,
            Some(field),
            &format!("unresolved reference GUID {guid}, fileID {id}"),
        ));
        json!({"$missing":{"guid":guid,"file_id":id}})
    }
    fn invalid_guid(&mut self, value: &Value, record: &Record, field: &str) -> bool {
        if value.get("guid").is_some_and(|guid| {
            guid.as_str()
                .is_none_or(|text| text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()))
        }) {
            self.diagnostics.push(diagnostic(
                record,
                "error",
                "invalid_reference_guid",
                Some(field),
                "guid must be a 32-character hexadecimal string",
            ));
            return true;
        }
        false
    }
    fn source_defect(&mut self, value: &Value, record: &Record, field: &str, id: i64) {
        let guid = value["guid"].as_str().unwrap_or(&record.source.guid);
        if self.lookup(value, record, field, id).is_none() {
            self.diagnostics.push(diagnostic(
                record, "warning", "source_reference_missing", Some(field),
                &format!("original reference GUID {guid}, fileID {id} is absent; explicit substitution applied"),
            ));
        }
    }
    fn lookup(&self, value: &Value, record: &Record, field: &str, id: i64) -> Option<Value> {
        let guid = value["guid"].as_str().unwrap_or(&record.source.guid);
        if field == "m_Script"
            && let Some(name) = self
                .profile
                .script_types
                .get(&format!("{guid}:{id}"))
                .or_else(|| self.profile.script_types.get(guid))
        {
            return Some(json!({"$type":name, "guid":guid,"file_id":id}));
        }
        let scope = value
            .get("_instance")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                if guid == record.source.guid {
                    &record.source.instance
                } else {
                    ""
                }
            });
        let key = SourceRef {
            guid: guid.into(),
            file_id: id,
            instance: scope.into(),
        }
        .key();
        let target = self.expansion.aliases.get(&key).unwrap_or(&key);
        if self.expansion.removed.contains(target) {
            return Some(Value::Null);
        }
        if self.expansion.records.contains_key(target) {
            return Some(json!({"$ref":target, "guid":guid, "file_id":id}));
        }
        if let Some(name) = self
            .profile
            .builtin_replacements
            .get(&format!("{guid}:{id}"))
            .or_else(|| self.profile.builtin_replacements.get(guid))
        {
            return Some(json!({"$builtin":name, "guid":guid, "file_id":id}));
        }
        let asset = self.project.assets.get(guid)?;
        let texture_id_exists = asset.kind != AssetKind::Texture
            || id == 2_800_000
            || asset.sprites.iter().any(|s| s.file_id == id);
        if texture_id_exists
            && (!self.project.docs.contains_key(guid)
                || (id == 100_100_000 && asset.kind == AssetKind::Prefab))
        {
            return Some(
                json!({"$ref":format!("{guid}:{id}"), "guid":guid, "file_id":id, "asset":asset.logical_id}),
            );
        }
        None
    }
}
pub(crate) fn reference(value: &Value) -> Option<String> {
    value["$ref"].as_str().map(str::to_owned)
}
