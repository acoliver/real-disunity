use crate::DisunityWorld;
use serde_json::{Value, json};

pub(super) fn snapshot(world: &DisunityWorld) -> Value {
    let mut scripts = Vec::new();
    let mut components = Vec::new();
    for component in world.components.iter().flatten() {
        components.push(json!({"handle":component.handle,"object":component.object}));
        if component.kind == "Script" {
            scripts.push(json!({
                "handle":component.handle,
                "object":component.object,
                "active_in_hierarchy":world.active(component.object),
                "enabled":component.enabled,
            }));
        }
    }
    let mut value = json!({});
    value["scripts"] = Value::Array(scripts);
    value["components"] = Value::Array(components);
    value
}

// The tuple projection carries only the eligibility consumed by phase dispatch.
pub(super) fn scripts(world: &DisunityWorld) -> Value {
    Value::Array(
        world
            .components
            .iter()
            .flatten()
            .filter(|component| component.kind == "Script")
            .map(|component| {
                json!([
                    component.handle,
                    world.active(component.object),
                    component.enabled
                ])
            })
            .collect(),
    )
}
