use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum ReadRequest {
    #[serde(rename = "describe")]
    Describe {},
    #[serde(rename = "session.status")]
    SessionStatus {},
    #[serde(rename = "lifecycle.snapshot")]
    Lifecycle {},
    #[serde(rename = "lifecycle.scripts")]
    Scripts {},
    #[serde(rename = "object.alive")]
    ObjectAlive { object: u64 },
    #[serde(rename = "component.alive")]
    ComponentAlive { component: u64 },
    #[serde(rename = "object.get")]
    Object { object: u64 },
    #[serde(rename = "object.find")]
    Find {
        name: Option<String>,
        tag: Option<String>,
        kind: Option<String>,
        #[serde(default)]
        include_inactive: bool,
    },
    #[serde(rename = "transform.get")]
    Transform { object: u64 },
    #[serde(rename = "component.get")]
    Component { component: u64 },
    #[serde(rename = "component.list")]
    Components { object: u64, kind: Option<String> },
    #[serde(rename = "input.get")]
    Input {
        #[serde(default)]
        phase: crate::input::projection::Phase,
    },
    #[serde(rename = "input.status")]
    InputStatus {},
    #[serde(rename = "time.get")]
    Time {},
    #[serde(rename = "snapshot")]
    Snapshot {},
    #[serde(rename = "physics.overlap")]
    Overlap {
        point: [f32; 2],
        layer_mask: Option<u32>,
        include_triggers: Option<bool>,
    },
    #[serde(rename = "physics.raycast")]
    Raycast {
        origin: [f32; 2],
        direction: [f32; 2],
        distance: f32,
        layer_mask: Option<u32>,
        include_triggers: Option<bool>,
    },
    #[serde(rename = "ui.rect")]
    Rect { object: u64 },
    #[serde(rename = "ui.layout")]
    Layout {},
}
impl ReadRequest {
    pub(super) fn execute(self, w: &DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Describe {} => Ok(super::describe::descriptor()),
            Self::SessionStatus {} => Ok(crate::strict::status(w)),
            Self::Lifecycle {} => Ok(super::lifecycle::snapshot(w)),
            Self::Scripts {} => Ok(super::lifecycle::scripts(w)),
            Self::ObjectAlive { object } => Ok(json!(w.object_index(object).is_ok())),
            Self::ComponentAlive { component } => Ok(json!(w.component_index(component).is_ok())),
            Self::Object { object } => w.object_snapshot(object),
            Self::Find {
                name,
                tag,
                kind,
                include_inactive,
            } => w.find_objects(name, tag, kind, include_inactive),
            Self::Transform { object } => w.transform_snapshot(object),
            Self::Component { component } => Ok(w.components[w.component_index(component)?]
                .as_ref()
                .unwrap()
                .snapshot()),
            Self::Components { object, kind } => components(w, object, kind),
            Self::Input { phase } => Ok(w.input.snapshot(phase)),
            Self::InputStatus {} => Ok(w.input.status()),
            Self::Time {} => Ok(json!(w.clock)),
            Self::Snapshot {} => snapshot(w),
            Self::Overlap {
                point,
                layer_mask,
                include_triggers,
            } => w.overlap_point(point, layer_mask, include_triggers),
            Self::Raycast {
                origin,
                direction,
                distance,
                layer_mask,
                include_triggers,
            } => w.raycast(origin, direction, distance, (layer_mask, include_triggers)),
            Self::Rect { object } => w.rect_snapshot(object),
            Self::Layout {} => w.ui_layout(),
        }
    }
}
fn components(w: &DisunityWorld, object: u64, kind: Option<String>) -> Result<Value, String> {
    w.object_index(object)?;
    Ok(json!(
        w.components
            .iter()
            .flatten()
            .filter(|c| c.object == object
                && kind.as_ref().is_none_or(|k| c.matches_kind(k)
                    || (c.kind == "Script" && c.fields.value()["type"].as_str() == Some(k))))
            .map(|c| c.snapshot())
            .collect::<Vec<_>>()
    ))
}
pub(crate) fn snapshot(w: &DisunityWorld) -> Result<Value, String> {
    let objects = w
        .objects
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            o.as_ref()
                .map(|_| w.object_snapshot(crate::make_handle(i, w.object_generations[i])))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut value =
        json!({"time":w.clock,"input":w.input.snapshot(crate::input::projection::Phase::Dynamic)});
    value["objects"] = Value::Array(objects);
    if w.strict.is_some() {
        value["session_stamp"] = crate::strict::status(w);
    }
    Ok(value)
}
