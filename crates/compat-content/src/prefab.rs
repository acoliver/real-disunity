use crate::{
    catalog::Project,
    model::{Diagnostic, ImportError, ImportProfile, ImportSidecar, SourceRef},
    property::{get_property, qualify, scalar_override, set_property},
    yaml::UnityDocument,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct Record {
    pub source: SourceRef,
    pub path: String,
    pub doc: UnityDocument,
    pub template: bool,
}
pub(crate) struct Expansion {
    pub records: BTreeMap<String, Record>,
    pub aliases: BTreeMap<String, String>,
    pub removed: BTreeSet<String>,
    pub diagnostics: Vec<Diagnostic>,
}
struct Scope<'a> {
    guid: &'a str,
    instance: &'a str,
    template: bool,
    depth: usize,
}
impl Scope<'_> {
    fn source(&self, file_id: i64) -> SourceRef {
        SourceRef {
            guid: self.guid.into(),
            file_id,
            instance: self.instance.into(),
        }
    }
}
impl Expansion {
    pub fn build(
        project: &Project,
        scene_guid: &str,
        profile: &ImportProfile,
        sidecar: &ImportSidecar,
    ) -> Result<Self, ImportError> {
        let mut result = Self {
            records: BTreeMap::new(),
            aliases: BTreeMap::new(),
            removed: BTreeSet::new(),
            diagnostics: vec![],
        };
        for guid in project.docs.keys() {
            result.expand(
                project,
                profile,
                &Scope {
                    guid,
                    instance: "",
                    template: guid != scene_guid,
                    depth: 0,
                },
            )?;
        }
        result.sidecars(sidecar, profile, project)?;
        Ok(result)
    }
    fn sidecars(
        &mut self,
        sidecar: &ImportSidecar,
        profile: &ImportProfile,
        project: &Project,
    ) -> Result<(), ImportError> {
        for patch in &sidecar.field_overrides {
            if patch.reason.trim().is_empty() {
                return Err(ImportError::Source {
                    path: "sidecar".into(),
                    message: "field override requires reason".into(),
                });
            }
            let records: Vec<_> = self
                .records
                .values_mut()
                .filter(|r| r.source.guid == patch.guid && r.source.file_id == patch.file_id)
                .collect();
            if records.is_empty() {
                return Err(ImportError::Source {
                    path: "sidecar".into(),
                    message: format!("override target absent: {}:{}", patch.guid, patch.file_id),
                });
            }
            for r in records {
                crate::overlay::validate(patch, r, project)?;
                set_property(
                    &mut r.doc.fields,
                    &patch.property_path,
                    patch.value.clone(),
                    profile.max_documents,
                )
                .map_err(|message| ImportError::Source {
                    path: r.path.clone(),
                    message,
                })?;
                self.diagnostics.push(diagnostic(
                    r,
                    "info",
                    "sidecar_field_override",
                    Some(&patch.property_path),
                    &patch.reason,
                ));
            }
        }
        Ok(())
    }
    fn expand(
        &mut self,
        project: &Project,
        profile: &ImportProfile,
        scope: &Scope<'_>,
    ) -> Result<(), ImportError> {
        if scope.depth > profile.max_prefab_depth {
            return Err(ImportError::Limit("prefab nesting/cycle".into()));
        }
        let Some(docs) = project.docs.get(scope.guid) else {
            return Ok(());
        };
        let path = project.index[scope.guid].path.clone();
        for doc in docs.iter().filter(|d| !d.stripped) {
            let source = scope.source(doc.file_id);
            self.records.entry(source.key()).or_insert_with(|| Record {
                source,
                path: path.clone(),
                doc: doc.clone(),
                template: scope.template,
            });
        }
        if self.records.len() > profile.max_documents {
            return Err(ImportError::Limit("expanded documents".into()));
        }
        for instance in docs.iter().filter(|d| d.class_id == 1001) {
            self.instance(project, profile, scope, instance)?;
        }
        for doc in docs.iter().filter(|d| d.stripped) {
            let corresponding = profile.correspondence_fields.iter().find_map(|f| {
                doc.fields
                    .get(f)
                    .filter(|r| r["fileID"].as_i64().is_some_and(|v| v != 0))
            });
            let instance = profile.instance_fields.iter().find_map(|f| {
                doc.fields
                    .get(f)
                    .and_then(|v| v["fileID"].as_i64())
                    .filter(|v| *v != 0)
            });
            if let (Some(target), Some(instance_id)) = (corresponding, instance) {
                let target = SourceRef {
                    guid: target["guid"].as_str().unwrap_or(scope.guid).into(),
                    file_id: target["fileID"].as_i64().unwrap_or(0),
                    instance: scope.source(instance_id).key(),
                }
                .key();
                self.aliases.insert(scope.source(doc.file_id).key(), target);
            }
        }
        Ok(())
    }
    fn instance(
        &mut self,
        project: &Project,
        profile: &ImportProfile,
        scope: &Scope<'_>,
        instance: &UnityDocument,
    ) -> Result<(), ImportError> {
        let source = profile.prefab_source_fields.iter().find_map(|f| {
            instance
                .fields
                .get(f)
                .filter(|r| r["fileID"].as_i64().is_some_and(|v| v != 0))
        });
        let Some(prefab_guid) = source.and_then(|s| s["guid"].as_str()) else {
            return Ok(());
        };
        let instance_key = scope.source(instance.file_id).key();
        if !project.docs.contains_key(prefab_guid) {
            self.diagnostics.push(diagnostic(
                &self.records[&instance_key],
                "error",
                "prefab_source_missing",
                None,
                &format!("prefab GUID {prefab_guid} has no source documents"),
            ));
            return Ok(());
        }
        self.expand(
            project,
            profile,
            &Scope {
                guid: prefab_guid,
                instance: &instance_key,
                template: scope.template,
                depth: scope.depth + 1,
            },
        )?;
        self.overrides(instance, scope, &instance_key, prefab_guid, profile);
        self.structure(instance, scope, &instance_key, prefab_guid);
        Ok(())
    }
    fn overrides(
        &mut self,
        instance: &UnityDocument,
        scope: &Scope<'_>,
        instance_key: &str,
        prefab_guid: &str,
        profile: &ImportProfile,
    ) {
        let Some(patches) = instance.fields["m_Modification"]["m_Modifications"].as_array() else {
            return;
        };
        for patch in patches {
            let target = SourceRef {
                guid: patch["target"]["guid"]
                    .as_str()
                    .unwrap_or(prefab_guid)
                    .into(),
                file_id: patch["target"]["fileID"].as_i64().unwrap_or(0),
                instance: instance_key.into(),
            }
            .key();
            let property = patch["propertyPath"].as_str().unwrap_or("");
            let Some(record) = self.records.get_mut(&target) else {
                self.diagnostics.push(diagnostic(
                    &self.records[&scope.source(instance.file_id).key()],
                    "error",
                    "prefab_override_target_missing",
                    Some(property),
                    &format!("override target {target} is absent"),
                ));
                continue;
            };
            let reference = &patch["objectReference"];
            let value = if reference["fileID"].as_i64().is_some_and(|v| v != 0) {
                qualify(reference.clone(), scope.guid, scope.instance)
            } else {
                let current = get_property(&record.doc.fields, property);
                if current.is_some_and(|v| v.get("fileID").is_some()) {
                    json!({"fileID":0})
                } else {
                    scalar_override(&patch["value"], current)
                }
            };
            if let Err(message) = set_property(
                &mut record.doc.fields,
                property,
                value,
                profile.max_documents,
            ) {
                self.diagnostics.push(diagnostic(
                    record,
                    "error",
                    "prefab_override_invalid",
                    Some(property),
                    &message,
                ));
            }
        }
    }
    fn structure(
        &mut self,
        instance: &UnityDocument,
        scope: &Scope<'_>,
        instance_key: &str,
        prefab_guid: &str,
    ) {
        let modification = &instance.fields["m_Modification"];
        if let Some(parent) = modification.get("m_TransformParent") {
            for record in self.records.values_mut().filter(|r| {
                r.source.instance == instance_key
                    && matches!(r.doc.kind.as_str(), "Transform" | "RectTransform")
            }) {
                if record.doc.fields["m_Father"]["fileID"].as_i64() == Some(0) {
                    record.doc.fields["m_Father"] =
                        qualify(parent.clone(), scope.guid, scope.instance);
                }
            }
        }
        if let Some(removed) = modification["m_RemovedComponents"].as_array() {
            for item in removed {
                let id = SourceRef {
                    guid: item["guid"].as_str().unwrap_or(prefab_guid).into(),
                    file_id: item["fileID"].as_i64().unwrap_or(0),
                    instance: instance_key.into(),
                }
                .key();
                self.records.remove(&id);
                self.removed.insert(id);
            }
        }
        for field in [
            "m_RemovedGameObjects",
            "m_AddedGameObjects",
            "m_AddedComponents",
        ] {
            if modification[field]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                self.diagnostics.push(diagnostic(
                    &self.records[&scope.source(instance.file_id).key()],
                    "error",
                    "unsupported_prefab_operation",
                    Some(field),
                    "prefab structural operation requires admission support",
                ));
            }
        }
    }
}
pub(crate) fn diagnostic(
    record: &Record,
    severity: &str,
    code: &str,
    field: Option<&str>,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        severity: severity.into(),
        code: code.into(),
        path: record.path.clone(),
        guid: record.source.guid.clone(),
        file_id: Some(record.source.file_id),
        component: Some(record.doc.kind.clone()),
        field: field.map(str::to_owned),
        message: message.into(),
    }
}
