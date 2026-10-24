use super::{
    native::{Fixture, Session},
    observation::Observation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use worldrunner::managed::CapturedSnapshot;

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
    pub managed: Observation,
    captured: CapturedSnapshot,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Extraction {
    pub fn capture(fixture: &Fixture, session: &Session) -> Result<Self, String> {
        let before = Observation::read(fixture, session);
        let captured = fixture
            .host
            .capture_snapshot(session.world)
            .map_err(|e| e.to_string())?;
        let after = Observation::read(fixture, session);
        assert_eq!(
            before, after,
            "bounded primitive observation and native capture are readonly"
        );
        before.bind(captured.stamp())?;
        let stamp =
            serde_json::from_value(serde_json::to_value(captured.stamp()).unwrap()).unwrap();
        let source = Self {
            stamp,
            managed: before,
            captured,
        };
        source.validate(fixture, session, true)?;
        Ok(source)
    }

    pub fn validate(
        &self,
        fixture: &Fixture,
        session: &Session,
        current: bool,
    ) -> Result<bool, String> {
        if hash(self.raw()) != self.stamp.snapshot_sha256 {
            return Err("owned raw extraction digest changed".into());
        }
        self.managed.bind(self.captured.stamp())?;
        fixture
            .host
            .validate_snapshot(session.world, &self.captured, current)
            .map_err(|e| e.to_string())
    }

    pub fn match_observation(&self, observation: &Observation) -> Result<(), String> {
        observation.bind(self.captured.stamp())
    }

    pub fn raw(&self) -> &[u8] {
        self.captured.raw_snapshot().as_bytes()
    }

    pub fn captured_value(&self) -> &Value {
        self.captured.value()
    }

    pub fn position(&self, object: u64) -> Result<[f32; 3], String> {
        let row = self.captured.value()["objects"]
            .as_array()
            .ok_or("objects missing")?
            .iter()
            .find(|row| row["handle"].as_u64() == Some(object))
            .ok_or("object missing")?;
        let transform = &row["transform"];
        if transform["rotation"] != serde_json::json!([0.0, 0.0, 0.0, 1.0])
            || transform["scale"] != serde_json::json!([1.0, 1.0, 1.0])
        {
            return Err("fixture requires identity rotation and scale".into());
        }
        let xyz: [f64; 3] =
            serde_json::from_value(transform["position"].clone()).map_err(|e| e.to_string())?;
        let mut result = [0.0; 3];
        for (index, value) in xyz.into_iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let projected = value as f32;
            if !value.is_finite() || f64::from(projected).to_bits() != value.to_bits() {
                return Err("native coordinate is not an exact finite f32 projection".into());
            }
            result[index] = projected;
        }
        Ok(result)
    }

    pub fn release(&self, fixture: &Fixture, session: &Session) -> Result<(), String> {
        fixture
            .host
            .release_snapshot(session.world, &self.captured)
            .map_err(|e| e.to_string())
    }
}
