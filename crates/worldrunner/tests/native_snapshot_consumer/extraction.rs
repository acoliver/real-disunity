use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::native::{Fixture, Session};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub world: u64,
    pub world_generation: u32,
    pub session: String,
    pub frame: u64,
    pub fixed_tick: u64,
    pub schedule_revision: u64,
    pub content_revision: u64,
    pub snapshot_id: u64,
    pub snapshot_sha256: String,
}

pub struct Extraction {
    pub stamp: Stamp,
    snapshot: Value,
    compact_fixture_bytes: Vec<u8>,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Extraction {
    pub fn capture(fixture: &Fixture, session: &Session) -> Result<Self, String> {
        let captured = fixture.control(session, json!({"op":"snapshot.capture"}));
        Self::decode(&captured)
    }

    pub fn decode(captured: &Value) -> Result<Self, String> {
        let stamp: Stamp =
            serde_json::from_value(captured["stamp"].clone()).map_err(|error| error.to_string())?;
        let snapshot = captured["snapshot"].clone();
        // Only the exact-coordinate fixture is qualified through the Value-only runtime API.
        let compact_fixture_bytes =
            serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
        if hash(&compact_fixture_bytes) != stamp.snapshot_sha256 {
            return Err("snapshot digest mismatch: raw-preserving runtime codec required".into());
        }
        Ok(Self {
            stamp,
            snapshot,
            compact_fixture_bytes,
        })
    }

    pub fn validate(
        &self,
        fixture: &Fixture,
        session: &Session,
        current: bool,
    ) -> Result<Value, String> {
        if hash(&self.compact_fixture_bytes) != self.stamp.snapshot_sha256 {
            return Err("owned extraction digest changed".into());
        }
        fixture
            .try_control(
                session,
                json!({"op":"snapshot.validate","stamp":self.stamp,"require_current":current}),
            )
            .map_err(|error| error.to_string())
    }

    pub fn position(&self, object: u64) -> Result<[f32; 3], String> {
        let row = self.snapshot["objects"]
            .as_array()
            .ok_or("objects missing")?
            .iter()
            .find(|row| row["handle"].as_u64() == Some(object))
            .ok_or("object missing")?;
        let transform = &row["transform"];
        if transform["rotation"] != json!([0.0, 0.0, 0.0, 1.0])
            || transform["scale"] != json!([1.0, 1.0, 1.0])
        {
            return Err("fixture requires identity rotation and scale".into());
        }
        let xyz: [f64; 3] = serde_json::from_value(transform["position"].clone())
            .map_err(|error| error.to_string())?;
        let mut projected = [0.0; 3];
        for (index, number) in xyz.into_iter().enumerate() {
            projected[index] = match number {
                0.0 => 0.0,
                0.5 => 0.5,
                1.0 => 1.0,
                _ => return Err("coordinate outside exact fixture codec".into()),
            };
        }
        Ok(projected)
    }

    pub fn captured_value(&self) -> Value {
        json!({"stamp":self.stamp,"snapshot":self.snapshot})
    }

    pub fn release(&self, fixture: &Fixture, session: &Session) {
        fixture.control(session, json!({"op":"snapshot.release","stamp":self.stamp}));
    }
}
