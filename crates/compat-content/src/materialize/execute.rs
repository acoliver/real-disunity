use crate::{
    MaterializationError, MaterializationPlan, MaterializedScene, NativeCommand, SpawnNode,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Display};

impl MaterializationPlan {
    /// Executes into the caller's world through one native protocol image.
    /// The closure returns unwrapped response data, as `CompatibilityHost::request` does.
    /// On failure, discard the partial world. Commands are never retried.
    pub fn execute<E: Display>(
        &self,
        mut request: impl FnMut(Value) -> Result<Value, E>,
    ) -> Result<MaterializedScene, MaterializationError> {
        let mut result = MaterializedScene {
            skipped_components: self.skipped_components.clone(),
            ..MaterializedScene::default()
        };
        for (index, command) in self.commands.iter().enumerate() {
            execute(command, &self.aliases, &mut result, &mut |value| {
                let data = request(value).map_err(|e| e.to_string())?;
                if data.get("ok").is_some() {
                    return Err(
                        "closure must return unwrapped response data; protocol envelope received"
                            .into(),
                    );
                }
                Ok(data)
            })
            .map_err(|message| MaterializationError::Native {
                command: index,
                message,
            })?;
        }
        for (alias, target) in &self.aliases {
            alias_map(alias, target, &mut result.handles);
            alias_map(alias, target, &mut result.object_ids);
            alias_map(alias, target, &mut result.component_ids);
            alias_map(alias, target, &mut result.transform_objects);
        }
        Ok(result)
    }
}
fn alias_map(alias: &str, target: &str, map: &mut BTreeMap<String, u64>) {
    if let Some(handle) = map.get(target).copied() {
        map.insert(alias.into(), handle);
    }
}
fn execute(
    command: &NativeCommand,
    aliases: &BTreeMap<String, String>,
    result: &mut MaterializedScene,
    request: &mut impl FnMut(Value) -> Result<Value, String>,
) -> Result<(), String> {
    match command {
        NativeCommand::CreateObject { node, transform_id } => {
            create(node, transform_id, result, request)
        }
        NativeCommand::AddComponent {
            id,
            object,
            kind,
            fields,
        } => {
            let mut allocation = fields.clone();
            if kind == "Canvas" {
                allocation["camera"] = Value::Null;
                allocation["render_mode"] = json!("screen_space_overlay");
            }
            let data = request(
                json!({"op":"component.add","object":handle(&result.object_ids,object)?,"kind":kind,"fields":allocation}),
            )?;
            let native = response_handle(&data)?;
            if kind == "RectTransform" {
                if result.handles.get(id) != Some(&native) {
                    return Err("RectTransform upgrade changed implicit Transform identity".into());
                }
            } else {
                insert(&mut result.handles, id, native)?;
            }
            result.component_ids.insert(id.clone(), native);
            Ok(())
        }
        _ => {
            let value = configuration(command, aliases, result)?;
            request(value)?;
            Ok(())
        }
    }
}
fn configuration(
    command: &NativeCommand,
    aliases: &BTreeMap<String, String>,
    result: &MaterializedScene,
) -> Result<Value, String> {
    match command {
        NativeCommand::Parent { object, parent } => Ok(json!({
            "op":"transform.parent", "object":handle(&result.object_ids,object)?,
            "parent":parent.as_ref().map(|id| handle(&result.object_ids,id)).transpose()?, "world_position_stays":false
        })),
        NativeCommand::Transform { object, transform } => Ok(json!({
            "op":"transform.set", "object":handle(&result.object_ids,object)?, "space":"local",
            "position":transform.local_position,"rotation":transform.local_rotation,"scale":transform.local_scale
        })),
        NativeCommand::ConfigureComponent {
            id,
            enabled,
            fields,
        } => {
            let mut fields = fields.clone();
            remap(&mut fields, aliases, &result.handles);
            Ok(
                json!({"op":"component.set","component":handle(&result.component_ids,id)?,"enabled":enabled,"fields":fields}),
            )
        }
        NativeCommand::Activate { object, active } => Ok(
            json!({"op":"object.set","object":handle(&result.object_ids,object)?,"active":active}),
        ),
        _ => Err("allocation command passed to configuration".into()),
    }
}
fn create(
    node: &SpawnNode,
    transform_id: &str,
    result: &mut MaterializedScene,
    request: &mut impl FnMut(Value) -> Result<Value, String>,
) -> Result<(), String> {
    let data =
        request(json!({"op":"object.create","name":node.name,"tag":node.tag,"active":false}))?;
    let object = response_handle(&data)?;
    let transform = data["components"]
        .as_array()
        .and_then(|components| components.iter().find(|c| c["kind"] == "Transform"))
        .ok_or("object.create omitted implicit Transform snapshot")?;
    let transform = response_handle(transform)?;
    insert(&mut result.handles, &node.id, object)?;
    insert(&mut result.handles, transform_id, transform)?;
    result.object_ids.insert(node.id.clone(), object);
    result.component_ids.insert(transform_id.into(), transform);
    result.transform_objects.insert(transform_id.into(), object);
    if node.template {
        result.template_active.insert(node.id.clone(), node.active);
    }
    Ok(())
}
fn insert(map: &mut BTreeMap<String, u64>, id: &str, native: u64) -> Result<(), String> {
    if map.contains_key(id) || map.values().any(|h| *h == native) {
        return Err(format!("duplicate source or native handle for {id}"));
    }
    map.insert(id.into(), native);
    Ok(())
}
fn response_handle(value: &Value) -> Result<u64, String> {
    value["handle"]
        .as_u64()
        .filter(|h| *h != 0)
        .ok_or_else(|| "native response omitted nonzero unsigned integer handle".into())
}
fn handle(map: &BTreeMap<String, u64>, id: &str) -> Result<u64, String> {
    map.get(id)
        .copied()
        .ok_or_else(|| format!("native identity absent: {id}"))
}
fn remap(value: &mut Value, aliases: &BTreeMap<String, String>, handles: &BTreeMap<String, u64>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map.get("$ref").and_then(Value::as_str) {
                let id = aliases.get(id).map_or(id, String::as_str);
                if let Some(native) = handles.get(id) {
                    map.insert("$ref".into(), json!(native));
                }
            }
            for v in map.values_mut() {
                remap(v, aliases, handles);
            }
        }
        Value::Array(array) => {
            for v in array {
                remap(v, aliases, handles);
            }
        }
        _ => {}
    }
}
