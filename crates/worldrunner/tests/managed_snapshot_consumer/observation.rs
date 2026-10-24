use super::native::{Fixture, Session};
use serde_json::{Value, json};
use worldrunner::managed::SnapshotStamp;

#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub managed: Value,
    pub roots: Value,
    pub native: Value,
    pub status: Value,
}

impl Observation {
    pub fn read(fixture: &Fixture, session: &Session) -> Self {
        let managed = fixture
            .host
            .observe(&json!({"op":"managed.snapshot"}))
            .unwrap()["data"]
            .clone();
        let roots = fixture.statistics();
        let mut native = fixture.request(session.world, &json!({"op":"snapshot"}));
        let mut status = fixture.request(session.world, &json!({"op":"session.status"}));
        // Snapshot capture changes only its retention ledger, outside simulation state.
        for value in [&mut status, &mut native["session_stamp"]] {
            for key in ["snapshot_receipts", "snapshot_bytes"] {
                value.as_object_mut().unwrap().remove(key);
            }
        }
        assert_eq!(status["stage"], "Idle");
        assert!(status["active_phase"].is_null());
        assert_eq!(managed["lifecycle_authority"], "native_readiness_single_v1");
        assert_eq!(managed["instances"].as_array().unwrap().len(), 1);
        assert_eq!(managed["instances"][0]["component"], session.script);
        assert_eq!(managed["instances"][0]["object"], session.object);
        assert_eq!(roots["scripts"], 1);
        assert_eq!(roots["coroutines"], 0);
        assert!(roots["bindings"].as_u64().unwrap() > 0);
        assert_eq!(roots["callback_failures"], 0);
        let observation = Self {
            managed,
            roots,
            native,
            status,
        };
        let text = observation.metadata().to_string();
        for forbidden in ["host_key", "initial_context", "\"token\""] {
            assert!(
                !text.contains(forbidden),
                "private authority leaked into observation"
            );
        }
        observation
    }

    pub fn fields(&self) -> &Value {
        &self.managed["instances"][0]["fields"]
    }

    pub fn bind(&self, stamp: &SnapshotStamp) -> Result<(), String> {
        let status = &self.status;
        for (key, value) in [
            ("world", stamp.world()),
            ("frame", stamp.frame()),
            ("fixed_tick", stamp.fixed_tick()),
            ("content_revision", stamp.content_revision()),
            ("revision", stamp.schedule_revision()),
        ] {
            if status[key].as_u64() != Some(value) {
                return Err(format!("managed observation/native source {key} mismatch"));
            }
        }
        if status["session"].as_str() != Some(stamp.session()) {
            return Err("managed observation/native session mismatch".into());
        }
        if self.fields()["updates"].as_u64() != Some(stamp.frame()) {
            return Err("fixture managed counter/native frame mismatch".into());
        }
        Ok(())
    }

    pub fn metadata(&self) -> Value {
        json!({"managed_primitive_fixture":self.managed,"root_statistics":self.roots,
            "native_readonly_snapshot":self.native,"native_status":self.status,
            "authority":"single-thread host correlation; native does not attest CLR fields"})
    }
}
