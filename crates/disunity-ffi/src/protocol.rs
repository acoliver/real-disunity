mod describe;
mod envelope;
mod transaction;
pub(crate) use envelope::Envelope;
mod guard;
mod lifecycle;
pub(crate) mod read;
mod strict;
mod timeline;

use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::{Value, json};

fn empty() -> Value {
    json!({})
}
fn local() -> String {
    "local".into()
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum Request {
    Read(read::ReadRequest),
    Transport(crate::transport::Request),
    Strict(strict::StrictRequest),
    Readiness(crate::readiness::ReadinessRequest),
    Callbacks(crate::readiness::batches::Request),
    Snapshot(crate::snapshots::SnapshotRequest),
    Transaction(transaction::Transaction),
    Object(ObjectRequest),
    Lifetime(crate::destruction::LifetimeRequest),
    Transform(TransformRequest),
    Component(ComponentRequest),
    Clock(ClockRequest),
    Timeline(timeline::TimelineRequest),
    Physics(PhysicsRequest),
    Ui(crate::ui::UiRequest),
}
impl Request {
    pub(crate) fn prepare(&mut self) -> Result<(), String> {
        if let Self::Strict(r) = self {
            r.prepare()?;
        }
        Ok(())
    }
    pub(crate) fn is_linear(&self) -> bool {
        matches!(
            self,
            Self::Strict(_)
                | Self::Readiness(_)
                | Self::Callbacks(_)
                | Self::Snapshot(_)
                | Self::Transport(_)
        )
    }
    pub(crate) fn global_descriptor(&self) -> Option<Value> {
        matches!(self, Self::Read(read::ReadRequest::Describe {})).then(describe::descriptor)
    }
    pub(crate) fn is_read(&self) -> bool {
        matches!(self, Self::Read(_)) || matches!(self, Self::Transport(r) if r.is_read())
    }
    pub(crate) fn changes_content(&self) -> Result<bool, String> {
        match self {
            Self::Read(_)
            | Self::Transport(_)
            | Self::Snapshot(_)
            | Self::Physics(PhysicsRequest::Events { clear: false }) => Ok(false),
            Self::Callbacks(r) => Ok(r.changes_content()),
            Self::Transaction(r) => r.changes_content(),
            Self::Strict(_)
            | Self::Readiness(_)
            | Self::Object(_)
            | Self::Lifetime(_)
            | Self::Transform(_)
            | Self::Component(_)
            | Self::Clock(_)
            | Self::Timeline(_)
            | Self::Physics(_)
            | Self::Ui(_) => Ok(true),
        }
    }
    pub(crate) fn execute_with_limit(
        self,
        w: &mut DisunityWorld,
        world: u64,
        limit: usize,
    ) -> Result<Value, String> {
        guard::check(&self, w)?;
        let observe = self.changes_content()? && !self.is_linear();
        let result = match self {
            Self::Read(r) => r.execute(w),
            Self::Transport(_) => {
                Err("transport controls require a top-level session request".into())
            }
            Self::Transaction(r) => r.execute(w, world, limit),
            Self::Strict(r) => r.execute(w, world),
            Self::Readiness(r) => r.execute(w, world),
            Self::Callbacks(r) => r.execute(w, world),
            Self::Snapshot(r) => r.execute(w, world),
            Self::Object(r) => r.execute(w),
            Self::Lifetime(r) => r.execute(w),
            Self::Transform(r) => r.execute(w),
            Self::Component(r) => r.execute(w),
            Self::Clock(r) => r.execute(w),
            Self::Timeline(r) => r.execute(w),
            Self::Physics(r) => r.execute(w),
            Self::Ui(r) => r.execute(w),
        };
        if observe && result.is_ok() {
            crate::readiness::observe(w);
        }
        result
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum ObjectRequest {
    #[serde(rename = "object.create")]
    Create {
        name: Option<String>,
        tag: Option<String>,
        active: Option<bool>,
        layer: Option<u8>,
    },
    #[serde(rename = "object.set")]
    Set {
        object: u64,
        name: Option<String>,
        tag: Option<String>,
        active: Option<bool>,
        layer: Option<u8>,
    },
    #[serde(rename = "object.destroy")]
    Destroy { object: u64 },
    #[serde(rename = "object.clone")]
    Clone { object: u64, parent: Option<u64> },
}
impl ObjectRequest {
    fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Create {
                name,
                tag,
                active,
                layer,
            } => {
                let h = w.allocate_object();
                w.set_metadata(h, name, tag, active, layer)?;
                w.object_snapshot(h)
            }
            Self::Set {
                object,
                name,
                tag,
                active,
                layer,
            } => {
                w.set_metadata(object, name, tag, active, layer)?;
                w.object_snapshot(object)
            }
            Self::Destroy { object } => {
                w.destroy_object(object)?;
                Ok(Value::Null)
            }
            Self::Clone { object, parent } => w.clone_object(object, parent),
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum TransformRequest {
    #[serde(rename = "transform.set")]
    TransformSet {
        object: u64,
        position: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
        #[serde(default = "local")]
        space: String,
    },
    #[serde(rename = "transform.parent")]
    Parent {
        object: u64,
        parent: Option<u64>,
        #[serde(default = "yes")]
        world_position_stays: bool,
    },
}
impl TransformRequest {
    fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::TransformSet {
                object,
                position,
                rotation,
                scale,
                space,
            } => {
                w.set_trs(object, position, rotation, scale, &space)?;
                w.wake_transform(object)?;
                w.transform_snapshot(object)
            }
            Self::Parent {
                object,
                parent,
                world_position_stays,
            } => {
                w.parent(object, parent, world_position_stays)?;
                w.transform_snapshot(object)
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum ComponentRequest {
    #[serde(rename = "component.add")]
    Add {
        object: u64,
        kind: String,
        #[serde(default = "empty")]
        fields: Value,
    },
    #[serde(rename = "component.set")]
    ComponentSet {
        component: u64,
        enabled: Option<bool>,
        fields: Option<Value>,
    },
    #[serde(rename = "component.remove")]
    Remove { component: u64 },
}
impl ComponentRequest {
    fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Add {
                object,
                kind,
                fields,
            } => {
                let h = w.add_component(object, kind, fields)?;
                Ok(w.components[w.component_index(h)?]
                    .as_ref()
                    .unwrap()
                    .snapshot())
            }
            Self::ComponentSet {
                component,
                enabled,
                fields,
            } => w.patch_component(component, enabled, fields),
            Self::Remove { component } => {
                w.destroy_component(component)?;
                Ok(Value::Null)
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum ClockRequest {
    #[serde(rename = "input.set")]
    InputSet {
        keys: std::collections::BTreeSet<String>,
        #[serde(default)]
        axes: std::collections::BTreeMap<String, f32>,
    },
    #[serde(rename = "input.configure_axes")]
    InputConfigure {
        axes: std::collections::BTreeMap<String, crate::input::axes::Axis>,
    },
    #[serde(rename = "input.focus")]
    InputFocus { focused: bool },
    #[serde(rename = "time.set")]
    TimeSet {
        time_scale: Option<f32>,
        fixed_delta_time: Option<f32>,
    },
    #[serde(rename = "time.phase")]
    Phase { phase: String },
    #[serde(rename = "frame.begin")]
    Begin { dt: f32 },
    #[serde(rename = "random.seed")]
    Seed { seed: u64 },
    #[serde(rename = "random.range")]
    Range {
        min: f64,
        max: f64,
        #[serde(default)]
        integer: bool,
    },
}
impl ClockRequest {
    fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::InputSet { keys, axes } => w.input.set(keys, axes),
            Self::InputConfigure { axes } => w.input.configure(axes),
            Self::InputFocus { focused } => w.input.focus(focused),
            Self::TimeSet {
                time_scale,
                fixed_delta_time,
            } => w.clock.set(time_scale, fixed_delta_time),
            Self::Begin { dt } => w.clock.begin(dt),
            Self::Phase { phase } => w.clock.phase(&phase),
            Self::Seed { seed } => {
                w.random.state = seed;
                Ok(Value::Null)
            }
            Self::Range { min, max, integer } => w.random.range(min, max, integer),
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum PhysicsRequest {
    #[serde(rename = "body.move")]
    BodyMove { component: u64, position: [f32; 2] },
    #[serde(rename = "body.sleep")]
    BodySleep { component: u64 },
    #[serde(rename = "body.wake")]
    BodyWake { component: u64 },
    #[serde(rename = "body.force")]
    BodyForce {
        component: u64,
        force: [f32; 2],
        mode: String,
    },
    #[serde(rename = "physics.layers")]
    Layers {
        layer_a: u8,
        layer_b: u8,
        ignore: bool,
    },
    #[serde(rename = "physics.configure")]
    Configure { gravity: [f32; 2] },
    #[serde(rename = "physics.step")]
    Step { dt: Option<f32> },
    #[serde(rename = "events.read")]
    Events {
        #[serde(default = "yes")]
        clear: bool,
    },
}
impl PhysicsRequest {
    fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::BodyMove {
                component,
                position,
            } => w.move_body(component, position),
            Self::BodySleep { component } => w.sleep_body(component, true),
            Self::BodyWake { component } => w.sleep_body(component, false),
            Self::BodyForce {
                component,
                force,
                mode,
            } => w.force_body(component, force, &mode),
            Self::Layers {
                layer_a,
                layer_b,
                ignore,
            } => w.physics.set_layers(layer_a, layer_b, ignore),
            Self::Configure { gravity } => {
                if gravity.iter().any(|v| !v.is_finite()) {
                    return Err("gravity must be finite".into());
                }
                w.physics.gravity = gravity;
                Ok(Value::Null)
            }
            Self::Step { dt } => w.physics_step(dt.unwrap_or(w.clock.fixed_delta_time)),
            Self::Events { clear } => {
                let events = if clear {
                    std::mem::take(&mut w.physics.events)
                } else {
                    w.physics.events.clone()
                };
                Ok(json!(events))
            }
        }
    }
}

pub(crate) fn envelope(result: Result<Value, String>) -> Value {
    match result {
        Ok(data) => {
            let mut value = json!({"version":1,"ok":true});
            value["data"] = data;
            value
        }
        Err(message) => {
            json!({"version":1,"ok":false,"error":{"code":"invalid_request","message":message}})
        }
    }
}
