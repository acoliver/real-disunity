use serde_json::{Value, json};
use std::path::PathBuf;
use worldrunner::managed::{CompatibilityHost, ManagedHostConfig};

pub struct Fixture {
    pub host: &'static mut CompatibilityHost,
    worlds: Vec<u64>,
}

pub struct Session {
    pub world: u64,
    pub object: u64,
    pub script: u64,
}

impl Fixture {
    pub fn boot() -> Self {
        let managed = PathBuf::from(std::env::var_os("SNAPSHOT_BRIDGE_DIR").unwrap());
        let config = ManagedHostConfig {
            ffi_library: PathBuf::from(std::env::var_os("SNAPSHOT_PROVIDER").unwrap()),
            runtime_config: managed.join("CompatibilityBridge.runtimeconfig.json"),
            assembly: managed.join("CompatibilityBridge.dll"),
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        };
        // Native callbacks remain reachable from CLR/TLS teardown until process exit.
        let host = Box::leak(Box::new(CompatibilityHost::boot(&config).unwrap()));
        Self {
            host,
            worlds: Vec::new(),
        }
    }

    pub fn require_snapshots(&self) {
        let description = self.host.request(0, &json!({"op":"describe"})).unwrap();
        assert!(
            description["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("snapshot_primitives_v1")),
            "required snapshot_primitives_v1 absent"
        );
        assert_eq!(self.statistics()["bindings"], 0);
    }

    fn seeded_world(&mut self) -> u64 {
        let world = self.host.create_world().unwrap();
        self.worlds.push(world);
        self.request(world, &json!({"op":"random.seed","seed":123_456_789}));
        world
    }

    fn random_oracle(&mut self) -> [Value; 3] {
        let world = self.seeded_world();
        let values = std::array::from_fn(|_| {
            self.request(
                world,
                &json!({"op":"random.range","min":0,"max":1_000_000,"integer":true}),
            )
        });
        self.destroy(world);
        values
    }

    pub fn foreign_world(&mut self) -> u64 {
        let world = self.seeded_world();
        self.host.open_phase_session(world).unwrap();
        world
    }

    pub fn load(&mut self) -> Session {
        let expected = self.random_oracle();
        let world = self.foreign_world();
        assert!(
            self.host.capture_snapshot(world).is_err(),
            "uninitialized capture rejects"
        );
        let object = self
            .host
            .request(
                world,
                &json!({"op":"object.create","name":"retained-marker"}),
            )
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let script = self
            .host
            .request(
                world,
                &json!({"op":"component.add","object":object,
            "kind":"Script","fields":{"type":"RetainedMarker"}}),
            )
            .unwrap()["handle"]
            .as_u64()
            .unwrap();
        let assembly = PathBuf::from(std::env::var_os("SNAPSHOT_FIXTURE").unwrap());
        let manifest = json!({"world":world,"assemblies":[assembly],
            "requirements":{"execution_profile":"native_phase_context_v1","scheduler_profile":"native_readiness_single_v1"},
            "handles":{"owner":object,"script":script},
            "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
            "components":[{"id":"script","object":"owner","script":{"type_name":"RetainedMarker","fields":{"expectedFirst":expected[0],"expectedSecond":expected[1],"expectedFinal":expected[2]}}}]}});
        self.host.load_fixture(&manifest).unwrap();
        Session {
            world,
            object,
            script,
        }
    }

    pub fn request(&self, world: u64, request: &Value) -> Value {
        self.host.request(world, request).unwrap()
    }

    pub fn statistics(&self) -> Value {
        self.host
            .observe(&json!({"op":"managed.statistics"}))
            .unwrap()["data"]
            .clone()
    }

    pub fn advance(&mut self, session: &Session) {
        self.host.advance_rational(session.world, 1, 50).unwrap();
        assert_eq!(self.host.callback_failures(0).unwrap().total, 0);
    }

    pub fn destroy(&mut self, world: u64) {
        self.host.destroy_world(world).unwrap();
        self.worlds.retain(|owned| *owned != world);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.host.unload().unwrap();
        for world in self.worlds.drain(..) {
            self.host.destroy_world(world).unwrap();
        }
        assert_eq!(self.host.store_statistics().unwrap().0, 0);
        for name in ["scripts", "coroutines", "bindings"] {
            assert_eq!(self.statistics()[name], 0);
        }
    }
}
