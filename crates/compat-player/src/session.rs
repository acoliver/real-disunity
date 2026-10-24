use std::{collections::BTreeSet, path::Path};

use compat_kit::RunRequest;
use serde_json::{Value, json};
use worldrunner::compat_content::ImportedScene;
use worldrunner::managed::CompatibilityHost;

use crate::{admission, assembly};

pub struct Session {
    host: CompatibilityHost,
    strict: bool,
    input_domain: Option<String>,
    world: Option<u64>,
    pub(crate) graph: ImportedScene,
    assembly: std::path::PathBuf,
    profile: Value,
    admission: worldrunner::compat_content::AdmissionPolicy,
    pub(crate) events: Vec<Value>,
    held: BTreeSet<String>,
    axes: Value,
    input_dirty: bool,
    seed: u64,
    resources: Vec<Value>,
    viewport: [u32; 2],
}

impl Session {
    pub fn open(root: &Path, request: &RunRequest) -> Result<Self, String> {
        let mut session = Self::prepare(root, request)?;
        session.load()?;
        Ok(session)
    }
    pub(crate) fn prepare(root: &Path, request: &RunRequest) -> Result<Self, String> {
        let strict = crate::strict::admit(request)?;
        let assemblies = assembly::prepare(root, request)?;
        let host = CompatibilityHost::boot(&assemblies.host).map_err(|error| error.to_string())?;
        let graph = admission::import(&request.source_root, &request.metadata)?;
        std::fs::write(
            request.artifact_root.join("imported-scene.json"),
            serde_json::to_vec_pretty(&graph).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mut profile = request.metadata["managed_profile"].clone();
        if profile.is_null() {
            profile = json!({});
        }
        profile["id"] = json!(request.profile);
        Ok(Self {
            host,
            strict,
            input_domain: None,
            world: None,
            graph,
            assembly: assemblies.game,
            profile,
            admission: admission::configuration(
                &request.source_root,
                &request.metadata["admission_policy"],
            )?,
            events: Vec::new(),
            held: BTreeSet::new(),
            axes: json!({}),
            input_dirty: false,
            seed: request.seed,
            resources: crate::resources::commands(root, request)?,
            viewport: if request.metadata["viewport"].is_null() {
                [960, 720]
            } else {
                serde_json::from_value(request.metadata["viewport"].clone())
                    .map_err(|error| error.to_string())?
            },
        })
    }

    pub(crate) fn load(&mut self) -> Result<(), String> {
        let world = self
            .host
            .create_world()
            .map_err(|error| error.to_string())?;
        self.world = Some(world);
        if self.strict {
            self.host
                .open_phase_session(world)
                .map_err(|error| error.to_string())?;
        }
        self.request(json!({"op":"random.seed","seed":self.seed}))?;
        self.request(json!({"op":"ui.configure","viewport":self.viewport}))?;
        for command in &self.resources {
            self.request(command.clone())?;
        }
        let handles = admission::materialize(&self.host, world, &self.graph, &self.admission)?;
        let mut manifest = json!({"world":world,"assemblies":[self.assembly],"graph":self.graph,"handles":handles,"profile":self.profile});
        if self.strict {
            manifest["requirements"] = json!({"execution_profile":crate::strict::PROFILE});
        }
        self.host
            .load_fixture(&manifest)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn viewport(&mut self, viewport: [u32; 2]) -> Result<(), String> {
        if self.viewport != viewport {
            self.request(json!({"op":"ui.configure","viewport":viewport}))?;
            self.viewport = viewport;
        }
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.unload()?;
        self.events.clear();
        self.input_domain = None;
        self.held.clear();
        self.axes = json!({});
        self.input_dirty = false;
        self.load()
    }

    pub fn unload(&mut self) -> Result<(), String> {
        crate::teardown::drain(
            self,
            |session| session.host.unload().map_err(|error| error.to_string()),
            |session| {
                if let Some(world) = session.world {
                    session
                        .host
                        .destroy_world(world)
                        .map_err(|error| error.to_string())?;
                    session.world = None;
                }
                Ok(())
            },
        )
    }

    pub fn request(&self, mut request: Value) -> Result<Value, String> {
        request["version"] = json!(1);
        self.host
            .request(self.world.ok_or("fixture has no native world")?, &request)
            .map_err(|error| error.to_string())
    }

    pub fn input(&mut self, keys: Vec<String>, axes: Value) -> Result<(), String> {
        crate::strict::action_mode(self.strict, "input")?;
        self.request(json!({"op":"input.set","keys":keys,"axes":axes}))?;
        self.held = keys.into_iter().collect();
        self.axes = axes;
        self.input_dirty = true;
        Ok(())
    }

    pub fn advance(&mut self, dt: f64) -> Result<(), String> {
        crate::strict::action_mode(self.strict, "advance")?;
        if !self.input_dirty {
            self.request(json!({"op":"input.set","keys":self.held,"axes":self.axes}))?;
        }
        let clock = self.request(json!({"op":"frame.begin","dt":dt}))?;
        self.request(json!({"op":"lifetime.begin"}))?;
        let execution = self.advance_phases(&clock);
        let destruction = self.request(json!({"op":"lifetime.end"}));
        execution?;
        destruction?;
        self.input_dirty = false;
        Ok(())
    }

    pub fn advance_rational(&mut self, num: u64, den: u64) -> Result<(), String> {
        crate::strict::action_mode(self.strict, "advance_rational")?;
        crate::strict::ratio(&json!({"num":num,"den":den}))?;
        let report = self.host.advance_rational_report(
            self.world.ok_or("fixture has no native world")?,
            num,
            den,
        );
        self.events.extend_from_slice(report.contacts());
        report.status.map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn input_events(&mut self, events: &Value) -> Result<(), String> {
        crate::strict::action_mode(self.strict, "input_events")?;
        let events = crate::strict::events(events)?;
        let domain = events[0]["source"].as_str().ok_or("input source missing")?;
        if self
            .input_domain
            .as_deref()
            .is_some_and(|previous| previous != domain)
        {
            return Err("strict input cannot mix OS and replay domains".into());
        }
        self.request(json!({"op":"input.events","events":events}))?;
        self.input_domain = Some(domain.into());
        Ok(())
    }

    fn advance_phases(&mut self, clock: &Value) -> Result<(), String> {
        let steps = clock["fixed_steps"]
            .as_u64()
            .ok_or("native clock does not expose fixed-step admission")?;
        let fixed_dt = clock["fixed_delta_time"]
            .as_f64()
            .ok_or("native clock has no fixed delta")?;
        for _ in 0..steps {
            self.request(json!({"op":"time.phase","phase":"fixed"}))?;
            self.phase("FixedUpdate")?;
            self.request(json!({"op":"physics.step","dt":fixed_dt}))?;
            let events = self.request(json!({"op":"events.read"}))?;
            for event in events
                .as_array()
                .ok_or("native contacts must be an array")?
            {
                self.host
                    .invoke_contact(event)
                    .map_err(|error| error.to_string())?;
                self.events.push(event.clone());
            }
        }
        self.request(json!({"op":"time.phase","phase":"update"}))?;
        self.phase("Update")?;
        self.phase("LateUpdate")?;
        Ok(())
    }

    fn phase(&mut self, phase: &str) -> Result<(), String> {
        self.host
            .dispatch_phase(&json!({"phase":phase}))
            .map_err(|error| error.to_string())
    }

    pub fn observe(&self, selector: &str) -> Result<Value, String> {
        match selector {
            "world.snapshot" => self.request(json!({"op":"snapshot"})),
            "managed.snapshot" => {
                let envelope = self
                    .host
                    .observe(&json!({"op":"managed.snapshot"}))
                    .map_err(|error| error.to_string())?;
                envelope
                    .get("data")
                    .cloned()
                    .ok_or("managed observation has no data".into())
            }
            "events" => Ok(json!(self.events)),
            "managed.failures" => self.managed_observe(&json!({"op":"managed.failures"})),
            "logs" => self.managed_observe(&json!({"op":"managed.logs"})),
            value if value.starts_with("managed:") => {
                let (type_name, path) = value[8..]
                    .split_once(':')
                    .ok_or("managed selector requires type and public member path")?;
                self.managed_observe(&json!({"op":"managed.public","type":type_name,"path":path}))
            }
            value if value.starts_with("object:") => {
                let snapshot = self.observe("world.snapshot")?;
                snapshot["objects"]
                    .as_array()
                    .and_then(|objects| {
                        objects
                            .iter()
                            .find(|object| object["name"].as_str() == Some(&value[7..]))
                    })
                    .cloned()
                    .ok_or_else(|| format!("no native object matches selector {value}"))
            }
            value => Err(format!("unsupported observation selector {value}")),
        }
    }

    fn managed_observe(&self, request: &Value) -> Result<Value, String> {
        self.host
            .observe(request)
            .map_err(|error| error.to_string())?
            .get("data")
            .cloned()
            .ok_or("managed observation has no data".into())
    }

    pub(crate) fn ensure_no_callback_failures(&self) -> Result<(), String> {
        self.host
            .ensure_no_callback_failures()
            .map_err(|error| error.to_string())
    }

    pub fn statistics(&self) -> Result<(u64, u64), String> {
        self.host
            .store_statistics()
            .map_err(|error| error.to_string())
    }

    pub fn managed_roots(&self) -> Result<u64, String> {
        let observation = self
            .host
            .observe(&json!({"op":"managed.statistics"}))
            .map_err(|error| error.to_string())?;
        let data = &observation["data"];
        ["scripts", "coroutines", "bindings"]
            .iter()
            .try_fold(0_u64, |total, field| {
                let count = data[*field]
                    .as_u64()
                    .ok_or_else(|| format!("managed statistics omitted {field}"))?;
                total
                    .checked_add(count)
                    .ok_or("managed root count overflow".into())
            })
    }
}

#[cfg(test)]
pub(crate) mod callback_test {
    use super::*;
    use worldrunner::managed::ManagedHostConfig;

    pub(crate) fn session() -> Session {
        let (mut host, world, probe) = loaded_host();
        // The caller deliberately ignores an operation error. Worker finalization must still fail.
        let _ = host.dispatch_phase(&json!({"phase":"Update"}));
        Session {
            host,
            strict: false,
            input_domain: None,
            world: Some(world),
            graph: serde_json::from_value(json!({
                "scene":"callback-test","scene_guid":"test","nodes":[],"components":[],"assets":[],
                "documents":[],"aliases":{},"diagnostics":[],"source_hashes":{}}))
            .unwrap(),
            assembly: probe,
            profile: json!({}),
            admission: worldrunner::compat_content::AdmissionPolicy::default(),
            events: Vec::new(),
            held: BTreeSet::new(),
            axes: json!({}),
            input_dirty: false,
            seed: 1,
            resources: Vec::new(),
            viewport: [960, 720],
        }
    }
    fn loaded_host() -> (CompatibilityHost, u64, std::path::PathBuf) {
        let artifacts = crate::managed_test_artifacts::artifacts();
        let config = ManagedHostConfig {
            ffi_library: artifacts.native.clone(),
            runtime_config: artifacts.bridge().with_extension("runtimeconfig.json"),
            assembly: artifacts.bridge(),
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        };
        let mut host = CompatibilityHost::boot(&config).unwrap();
        let world = host.create_world().unwrap();
        let object = host
            .request(
                world,
                &json!({"op":"object.create","name":"worker-failure"}),
            )
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"CallbackIsolationProbe"}})).unwrap()["handle"].as_u64().unwrap();
        let probe = artifacts.probe();
        host.load_fixture(&json!({"world":world,"assemblies":[probe],"handles":{"owner":object,"script":script},"graph":{
            "nodes":[{"id":"owner","active":true,"template":false}],"assets":[],"aliases":{},
            "components":[{"id":"script","object":"owner","script":{"type_name":"CallbackIsolationProbe","fields":{"failure":"Update"}}}]}})).unwrap();
        (host, world, probe)
    }
}
