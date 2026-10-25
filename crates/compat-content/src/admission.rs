use crate::{
    AdmissionPolicy, AssetKind, ImportedComponent, ImportedScene, MaterializationError,
    MaterializationPlan, NativeCommand,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn error(id: &str, message: impl Into<String>) -> MaterializationError {
    MaterializationError::Graph {
        id: id.into(),
        message: message.into(),
    }
}
pub(crate) fn plan(
    scene: &ImportedScene,
    policy: &AdmissionPolicy,
) -> Result<MaterializationPlan, MaterializationError> {
    let skipped_components = omissions(scene, policy)?;
    let components: Vec<_> = scene
        .components
        .iter()
        .filter(|c| c.object.is_some() && !skipped_components.contains_key(&c.id))
        .collect();
    validate(scene, &components)?;
    let mut commands = Vec::new();
    for node in &scene.nodes {
        let transforms: Vec<_> = components
            .iter()
            .filter(|c| {
                c.object.as_ref() == Some(&node.id)
                    && matches!(
                        c.native_kind.as_deref(),
                        Some("Transform" | "RectTransform")
                    )
            })
            .collect();
        if transforms.len() != 1 {
            return Err(error(
                &node.id,
                "exactly one authored Transform is required",
            ));
        }
        commands.push(NativeCommand::CreateObject {
            node: Box::new(node.clone()),
            transform_id: transforms[0].id.clone(),
        });
    }
    for c in &components {
        if c.native_kind.as_deref() != Some("Transform") {
            commands.push(NativeCommand::AddComponent {
                id: c.id.clone(),
                object: c.object.clone().expect("owned component"),
                kind: c.native_kind.clone().expect("admitted kind"),
                fields: c.native_fields.clone(),
            });
        }
    }
    configure(scene, &components, &mut commands);
    Ok(MaterializationPlan {
        commands,
        skipped_components,
        aliases: scene.aliases.clone(),
    })
}
fn configure(
    scene: &ImportedScene,
    components: &[&ImportedComponent],
    commands: &mut Vec<NativeCommand>,
) {
    for c in components {
        if !matches!(
            c.native_kind.as_deref(),
            Some("Transform" | "RectTransform")
        ) {
            commands.push(NativeCommand::ConfigureComponent {
                id: c.id.clone(),
                enabled: c.enabled,
                fields: c.native_fields.clone(),
            });
        }
    }
    for node in &scene.nodes {
        commands.push(NativeCommand::Parent {
            object: node.id.clone(),
            parent: node.parent.clone(),
        });
        commands.push(NativeCommand::Transform {
            object: node.id.clone(),
            transform: node.transform.clone(),
        });
    }
    for c in components
        .iter()
        .filter(|c| c.native_kind.as_deref() == Some("RectTransform"))
    {
        commands.push(NativeCommand::ConfigureComponent {
            id: c.id.clone(),
            enabled: c.enabled,
            fields: c.native_fields.clone(),
        });
    }
    for node in &scene.nodes {
        commands.push(NativeCommand::Activate {
            object: node.id.clone(),
            active: node.active && !node.template,
        });
    }
}
fn omissions(
    scene: &ImportedScene,
    policy: &AdmissionPolicy,
) -> Result<BTreeMap<String, String>, MaterializationError> {
    let mut skipped = BTreeMap::new();
    for c in scene.components.iter().filter(|c| c.object.is_some()) {
        if let Some(reason) = omission(c, policy) {
            if reason.trim().is_empty()
                || matches!(
                    c.native_kind.as_deref(),
                    Some("Transform" | "RectTransform")
                )
            {
                return Err(error(
                    &c.id,
                    "omission requires a reason and cannot remove Transform",
                ));
            }
            skipped.insert(c.id.clone(), reason.to_owned());
        } else if c.native_kind.is_none() {
            return Err(error(
                &c.id,
                format!(
                    "{} has no native mapping or explicit omission policy",
                    c.kind
                ),
            ));
        }
    }
    Ok(skipped)
}
fn omission<'a>(c: &ImportedComponent, policy: &'a AdmissionPolicy) -> Option<&'a str> {
    let script = &c.fields["m_Script"];
    let script = script.get("$missing").unwrap_or(script);
    let key = script["guid"]
        .as_str()
        .zip(script["file_id"].as_i64())
        .map(|(guid, file)| format!("{guid}:{file}"));
    policy
        .ignored_components
        .get(&c.id)
        .or_else(|| key.as_ref().and_then(|k| policy.ignored_scripts.get(k)))
        .or_else(|| policy.ignored_components.get(&c.kind))
        .map(String::as_str)
}
fn validate(
    scene: &ImportedScene,
    components: &[&ImportedComponent],
) -> Result<(), MaterializationError> {
    let mut live = BTreeSet::new();
    for id in scene
        .nodes
        .iter()
        .map(|n| &n.id)
        .chain(components.iter().map(|c| &c.id))
    {
        if !live.insert(id.clone()) {
            return Err(error(id, "duplicate native identity"));
        }
    }
    hierarchy(scene)?;
    for (path, metadata) in &scene.metadata_records {
        MetadataValidation {
            path,
            scene,
            live: &live,
        }
        .check(metadata, 0)?;
    }
    for c in components {
        if !scene.nodes.iter().any(|n| c.object.as_ref() == Some(&n.id)) {
            return Err(error(&c.id, "component owner absent"));
        }
        if matches!(c.kind.as_str(), "Transform" | "RectTransform") {
            values(&c.fields["m_Father"], &c.id, scene, &live, 0)?;
        }
        values(&c.native_fields, &c.id, scene, &live, 0)?;
        if let Some(script) = &c.script {
            values(&script.fields, &c.id, scene, &live, 0)?;
        }
    }
    for c in scene.components.iter().filter(|c| c.object.is_none()) {
        if let Some(script) = &c.script {
            values(&script.fields, &c.id, scene, &live, 0)?;
        }
    }
    Ok(())
}
fn hierarchy(scene: &ImportedScene) -> Result<(), MaterializationError> {
    let nodes: BTreeMap<_, _> = scene.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for node in &scene.nodes {
        let mut seen = BTreeSet::from([node.id.as_str()]);
        let mut parent = node.parent.as_deref();
        while let Some(id) = parent {
            let ancestor = nodes
                .get(id)
                .ok_or_else(|| error(&node.id, format!("parent {id} absent")))?;
            if !seen.insert(id) || ancestor.template != node.template {
                return Err(error(
                    &node.id,
                    "parent cycle or scene/template hierarchy crossing",
                ));
            }
            parent = ancestor.parent.as_deref();
        }
    }
    Ok(())
}
fn values(
    value: &Value,
    owner: &str,
    scene: &ImportedScene,
    live: &BTreeSet<String>,
    depth: usize,
) -> Result<(), MaterializationError> {
    if depth > 96 {
        return Err(error(owner, "field recursion exceeds 96"));
    }
    if value.get("$missing").is_some() {
        return Err(error(
            owner,
            format!("required field contains unresolved reference: {value}"),
        ));
    }
    if let Some(reference) = value.get("$ref") {
        let id = reference
            .as_str()
            .ok_or_else(|| error(owner, "graph reference must be a stable string"))?;
        let target = scene.aliases.get(id).map_or(id, String::as_str);
        if !live.contains(target) && !is_data(target, scene) {
            return Err(error(
                owner,
                format!("required reference {id} has no admitted native or data identity"),
            ));
        }
        return Ok(());
    }
    match value {
        Value::Object(map) => {
            for v in map.values() {
                values(v, owner, scene, live, depth + 1)?;
            }
        }
        Value::Array(array) => {
            for v in array {
                values(v, owner, scene, live, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn is_data(id: &str, scene: &ImportedScene) -> bool {
    if scene.nodes.iter().any(|n| n.id == id) {
        return false;
    }
    if let Some(c) = scene.components.iter().find(|c| c.id == id) {
        return c.object.is_none();
    }
    if scene.documents.iter().any(|d| d.source.key() == id) {
        return true;
    }
    let Some((guid, file)) = id.split_once(':') else {
        return false;
    };
    let Ok(file) = file.parse::<i64>() else {
        return false;
    };
    let Some(asset) = scene.assets.iter().find(|a| a.guid == guid) else {
        return false;
    };
    if file == 0 {
        return false;
    }
    match asset.kind {
        AssetKind::Scene | AssetKind::ScriptableObject | AssetKind::Material => false,
        AssetKind::Prefab => file == 100_100_000,
        AssetKind::Texture => file == 2_800_000 || asset.sprites.iter().any(|s| s.file_id == file),
        _ => true,
    }
}

struct MetadataValidation<'a> {
    path: &'a str,
    scene: &'a ImportedScene,
    live: &'a BTreeSet<String>,
}
impl MetadataValidation<'_> {
    fn check(&self, value: &Value, depth: usize) -> Result<(), MaterializationError> {
        if depth > 96 {
            return Err(error(self.path, "field recursion exceeds 96"));
        }
        if value.get("$missing").is_some() || value.get("$ref").is_some() {
            return values(value, self.path, self.scene, self.live, depth);
        }
        match value {
            Value::Object(map) => {
                for value in map.values() {
                    self.check(value, depth + 1)?;
                }
            }
            Value::Array(array) => {
                for value in array {
                    self.check(value, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
