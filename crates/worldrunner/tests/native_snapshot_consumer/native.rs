use serde_json::{Value, json};
use std::path::PathBuf;
use worldrunner::managed::{CompatibilityHost, HostError, ManagedHostConfig};

pub struct Fixture {
    host: &'static mut CompatibilityHost,
    worlds: Vec<u64>,
}

pub struct Session {
    pub world: u64,
    authority: Value,
}

impl Fixture {
    pub fn boot() -> Self {
        let image =
            PathBuf::from(std::env::var_os("SNAPSHOT_PROVIDER").expect("explicit provider"));
        let managed =
            PathBuf::from(std::env::var_os("SNAPSHOT_BRIDGE_DIR").expect("explicit bridge"));
        let config = ManagedHostConfig {
            ffi_library: image,
            runtime_config: managed.join("CompatibilityBridge.runtimeconfig.json"),
            assembly: managed.join("CompatibilityBridge.dll"),
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        };
        // CLR retains native callbacks until process exit, including during TLS teardown.
        let host = Box::leak(Box::new(CompatibilityHost::boot(&config).unwrap()));
        Self {
            host,
            worlds: Vec::new(),
        }
    }

    pub fn require_snapshots(&self) {
        let description = self.request(0, &json!({"op":"describe"}));
        assert!(
            description["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("snapshot_primitives_v1")),
            "required snapshot_primitives_v1 absent: {description}"
        );
    }

    pub fn request(&self, world: u64, request: &Value) -> Value {
        self.host.request(world, request).unwrap()
    }

    pub fn try_control(&self, session: &Session, mut request: Value) -> Result<Value, HostError> {
        request["host"] = session.authority.clone();
        self.host.request(session.world, &request)
    }

    pub fn control(&self, session: &Session, request: Value) -> Value {
        self.try_control(session, request).unwrap()
    }

    pub fn open(&mut self) -> Session {
        let world = self.host.create_world().unwrap();
        self.worlds.push(world);
        let opened = self.request(
            world,
            &json!({"op":"session.open","profile":"native_phase_context_v1"}),
        );
        Session {
            world,
            authority: json!({"session":opened["session"],"host_key":opened["host_key"]}),
        }
    }

    pub fn phase(&self, session: &Session, phase: &str) -> Value {
        let status = self.request(session.world, &json!({"op":"session.status"}));
        let tick = status["fixed_tick"].as_u64().unwrap();
        let tick = if matches!(phase, "FixedUpdate" | "FixedCoroutines") {
            tick + 1
        } else {
            tick
        };
        let context = self.control(
            session,
            json!({"op":"phase.open","phase":phase,"frame":status["frame"],"fixed_tick":tick}),
        )["context"]
            .clone();
        self.request(
            session.world,
            &json!({"op":"phase.claim","phase":phase,"context":context}),
        );
        context
    }

    pub fn finish(&self, session: &Session, context: &Value) {
        self.control(session, json!({"op":"phase.finish","context":context}));
    }

    pub fn initialize(&self, session: &Session) -> u64 {
        assert!(
            self.try_control(session, json!({"op":"snapshot.capture"}))
                .is_err()
        );
        let context = self.phase(session, "Initialize");
        assert!(
            self.try_control(session, json!({"op":"snapshot.capture"}))
                .is_err()
        );
        let object = self.request(
            session.world,
            &json!({"op":"object.create","name":"native-marker"}),
        )["handle"]
            .as_u64()
            .unwrap();
        self.request(
            session.world,
            &json!({"op":"transform.set","object":object,"position":[0,0,0]}),
        );
        self.finish(session, &context);
        object
    }

    // Test-only native phase driver. No managed fixture or production scheduler is installed.
    pub fn advance(&self, session: &Session, object: u64, position: [f32; 3]) {
        let opened = self.control(session, json!({"op":"frame.open","dt":{"num":0,"den":1}}));
        assert_eq!(opened["time"]["fixed_steps"], 0);
        let context = self.phase(session, "Update");
        let before = self.revision(session);
        self.request(
            session.world,
            &json!({"op":"transform.set","object":object,"position":position}),
        );
        assert_eq!(
            self.revision(session),
            before + 1,
            "canonical TRS advances native content revision exactly once"
        );
        self.finish(session, &context);
        for phase in ["UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
            let context = self.phase(session, phase);
            self.finish(session, &context);
        }
        self.control(
            session,
            json!({"op":"frame.end","frame":opened["time"]["frame"]}),
        );
    }

    fn revision(&self, session: &Session) -> u64 {
        self.request(session.world, &json!({"op":"session.status"}))["content_revision"]
            .as_u64()
            .unwrap()
    }

    pub fn destroy(&mut self, session: &Session) {
        self.host.destroy_world(session.world).unwrap();
        self.worlds.retain(|world| *world != session.world);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for world in self.worlds.drain(..) {
            self.host.destroy_world(world).unwrap();
        }
        assert_eq!(
            self.host.store_statistics().unwrap().0,
            0,
            "all native objects drained"
        );
    }
}
