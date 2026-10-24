use crate::{
    binding,
    catalog::Project,
    model::{
        Diagnostic, ImportProfile, ImportSidecar, ImportedComponent, ImportedDocument,
        ImportedScene, LocalTransform, SpawnNode,
    },
    native,
    prefab::{Expansion, Record, diagnostic},
    refs::{Resolver, reference},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type DocumentFields = BTreeMap<String, Value>;

pub(crate) fn build(
    project: Project,
    expansion: Expansion,
    scene: String,
    scene_guid: String,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
) -> ImportedScene {
    let mut linker = Resolver {
        project: &project,
        expansion: &expansion,
        profile,
        sidecar,
        diagnostics: vec![],
    };
    let fields: DocumentFields = expansion
        .records
        .iter()
        .map(|(k, r)| (k.clone(), linker.fields(r)))
        .collect();
    let metadata_records = metadata_records(&mut linker, &scene_guid);
    let mut diagnostics = expansion.diagnostics.clone();
    diagnostics.extend(linker.diagnostics);
    let mut components: Vec<_> = expansion
        .records
        .values()
        .filter_map(|r| component(r, &fields, &project, profile, &mut diagnostics))
        .collect();
    let nodes = nodes(&expansion, &fields, &components);
    crate::tiles::prepare(&mut components, &nodes, &mut diagnostics);
    let documents = expansion
        .records
        .iter()
        .map(|(key, r)| ImportedDocument {
            source: r.source.clone(),
            path: r.path.clone(),
            class_id: r.doc.class_id,
            kind: r.doc.kind.clone(),
            fields: fields[key].clone(),
            template: r.template,
        })
        .collect();
    let mut scene = ImportedScene {
        scene,
        scene_guid,
        nodes,
        components,
        assets: project.assets.into_values().collect(),
        documents,
        aliases: expansion.aliases,
        diagnostics,
        source_hashes: project.hashes,
        metadata_records,
        fallback_sprites: sidecar.fallback_sprites.clone(),
    };
    crate::tile_collision::prepare(&mut scene, sidecar);
    normalize_diagnostics(&mut scene.diagnostics, &scene.assets);
    scene
}

fn metadata_records(linker: &mut Resolver<'_>, scene_guid: &str) -> DocumentFields {
    let mut metadata_records: BTreeMap<_, _> = linker
        .project
        .assets
        .values()
        .map(|asset| {
            let path = format!("{}.meta", asset.path);
            let record = Record {
                source: crate::SourceRef {
                    guid: asset.guid.clone(),
                    file_id: 0,
                    instance: String::new(),
                },
                path: path.clone(),
                template: true,
                doc: crate::yaml::UnityDocument {
                    class_id: 0,
                    file_id: 0,
                    stripped: false,
                    kind: "Metadata".into(),
                    fields: asset.metadata.clone(),
                },
            };
            (path, linker.fields(&record))
        })
        .collect();
    for (path, fields) in &linker.project.settings {
        let record = Record {
            source: crate::SourceRef {
                guid: scene_guid.into(),
                file_id: 0,
                instance: String::new(),
            },
            path: path.clone(),
            template: true,
            doc: crate::yaml::UnityDocument {
                class_id: 0,
                file_id: 0,
                stripped: false,
                kind: "Settings".into(),
                fields: serde_json::json!({"documents":fields}),
            },
        };
        metadata_records.insert(path.clone(), linker.fields(&record));
    }
    metadata_records
}

fn normalize_diagnostics(diagnostics: &mut Vec<Diagnostic>, assets: &[crate::ImportedAsset]) {
    for diagnostic in diagnostics.iter_mut() {
        if diagnostic.path.is_empty() {
            diagnostic.path = assets
                .iter()
                .find(|asset| asset.guid == diagnostic.guid)
                .map_or_else(String::new, |asset| asset.path.clone());
        }
    }
    diagnostics.sort_by(|a, b| {
        (&a.path, a.file_id, &a.field, &a.code, &a.message)
            .cmp(&(&b.path, b.file_id, &b.field, &b.code, &b.message))
    });
    diagnostics.dedup_by(|a, b| {
        a.path == b.path
            && a.file_id == b.file_id
            && a.field == b.field
            && a.code == b.code
            && a.message == b.message
    });
}

fn component(
    record: &Record,
    documents: &DocumentFields,
    project: &Project,
    profile: &ImportProfile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ImportedComponent> {
    let id = record.source.key();
    let fields = &documents[&id];
    let object = reference(&fields["m_GameObject"]);
    if object.is_none() && record.doc.kind != "MonoBehaviour" {
        return None;
    }
    let script = binding::script(record, fields, project, profile, diagnostics);
    let (native_kind, mut native_fields) = native::convert(
        &record.doc.kind,
        fields,
        script.as_ref().map(|s| s.type_name.as_str()),
    );
    if native_kind.as_deref() == Some("Script")
        && let Some(script) = &script
        && let Some(map) = native_fields.as_object_mut()
    {
        map.extend(script.fields.as_object().cloned().unwrap_or_default());
    }
    apply_material(&record.doc.kind, fields, &mut native_fields, documents);
    if native_kind.is_none() && object.is_some() {
        diagnostics.push(diagnostic(
            record,
            "warning",
            "unsupported_component",
            None,
            "component preserved in graph; no native admission mapping",
        ));
    }
    if record.doc.kind == "MonoBehaviour" && script.is_none() {
        diagnostics.push(diagnostic(
            record,
            "error",
            "script_type_unavailable",
            Some("m_Script"),
            &format!(
                "script {} has no original source or configured managed type",
                record.doc.fields["m_Script"]
            ),
        ));
    }
    Some(ImportedComponent {
        id,
        source: record.source.clone(),
        object,
        kind: record.doc.kind.clone(),
        class_id: record.doc.class_id,
        enabled: native::boolean(&fields["m_Enabled"], true),
        fields: fields.clone(),
        native_kind,
        native_fields,
        script,
    })
}

fn apply_material(
    kind: &str,
    fields: &Value,
    native_fields: &mut Value,
    documents: &DocumentFields,
) {
    if !matches!(
        kind,
        "Rigidbody2D"
            | "BoxCollider2D"
            | "CircleCollider2D"
            | "PolygonCollider2D"
            | "TilemapCollider2D"
    ) {
        return;
    }
    let Some(material) = reference(&fields["m_Material"]).and_then(|id| documents.get(&id)) else {
        return;
    };
    let bounciness = material
        .get("m_Bounciness")
        .or_else(|| material.get("bounciness"))
        .cloned()
        .unwrap_or(json!(0.));
    let friction = material
        .get("m_Friction")
        .or_else(|| material.get("friction"))
        .cloned()
        .unwrap_or(json!(0.4));
    native_fields["material"] =
        json!({"bounciness":bounciness,"friction":friction,"asset":fields["m_Material"]});
    if kind == "Rigidbody2D" {
        native_fields["restitution"] = bounciness;
    }
}

fn nodes(
    expansion: &Expansion,
    fields: &DocumentFields,
    components: &[ImportedComponent],
) -> Vec<SpawnNode> {
    let owners: BTreeMap<_, _> = components
        .iter()
        .filter_map(|c| c.object.as_ref().map(|o| (c.id.clone(), o.clone())))
        .collect();
    let mut by_owner: BTreeMap<&str, Vec<&ImportedComponent>> = BTreeMap::new();
    for component in components {
        if let Some(object) = &component.object {
            by_owner.entry(object).or_default().push(component);
        }
    }
    let mut nodes = Vec::new();
    for (key, record) in &expansion.records {
        if record.doc.kind != "GameObject" {
            continue;
        }
        let values = &fields[key];
        let owned = by_owner.get(key.as_str()).cloned().unwrap_or_default();
        let transform = owned
            .iter()
            .find(|c| matches!(c.kind.as_str(), "Transform" | "RectTransform"));
        let parent = transform
            .and_then(|c| reference(&c.fields["m_Father"]))
            .and_then(|id| owners.get(&id).cloned());
        nodes.push(SpawnNode {
            id: key.clone(),
            source: record.source.clone(),
            name: values["m_Name"].as_str().unwrap_or("").into(),
            tag: values["m_TagString"].as_str().unwrap_or("Untagged").into(),
            active: native::boolean(&values["m_IsActive"], true),
            layer: values["m_Layer"].as_i64().unwrap_or(0),
            parent,
            children: vec![],
            components: owned.iter().map(|c| c.id.clone()).collect(),
            transform: local_transform(transform.map(|c| &c.fields)),
            template: record.template,
        });
    }
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in &nodes {
        if let Some(parent) = &node.parent {
            children
                .entry(parent.clone())
                .or_default()
                .push(node.id.clone());
        }
    }
    for node in &mut nodes {
        node.children = children.remove(&node.id).unwrap_or_default();
    }
    nodes
}
fn local_transform(fields: Option<&Value>) -> LocalTransform {
    fields.map_or_else(LocalTransform::default, |f| LocalTransform {
        local_position: native::vector(&f["m_LocalPosition"], ["x", "y", "z"], [0.; 3]),
        local_rotation: native::vector(
            &f["m_LocalRotation"],
            ["x", "y", "z", "w"],
            [0., 0., 0., 1.],
        ),
        local_scale: native::vector(&f["m_LocalScale"], ["x", "y", "z"], [1.; 3]),
    })
}
