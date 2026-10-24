use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
use sha2::{Digest, Sha256};

use super::{CompatibilityHost, RESPONSE_LIMIT};
use crate::HostError;

/// Public receipt identity. Native retained metadata supplies its authority.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotStamp {
    world: u64,
    world_generation: u32,
    session: String,
    frame: u64,
    fixed_tick: u64,
    schedule_revision: u64,
    content_revision: u64,
    snapshot_id: u64,
    snapshot_sha256: String,
}

impl SnapshotStamp {
    pub fn world(&self) -> u64 {
        self.world
    }
    pub fn world_generation(&self) -> u32 {
        self.world_generation
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn fixed_tick(&self) -> u64 {
        self.fixed_tick
    }
    pub fn schedule_revision(&self) -> u64 {
        self.schedule_revision
    }
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }
    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }
}

/// Exact native bytes and their immutable, exact integer/binary64 projection.
/// This owns no native image, response handle, host key or second world store.
pub struct CapturedSnapshot {
    raw: Box<RawValue>,
    stamp: SnapshotStamp,
    value: Value,
}

impl CapturedSnapshot {
    pub fn raw_snapshot(&self) -> &str {
        self.raw.get()
    }
    pub fn stamp(&self) -> &super::SnapshotStamp {
        &self.stamp
    }
    pub fn value(&self) -> &Value {
        &self.value
    }

    fn parse(text: &str) -> Result<Self, HostError> {
        if text.len() > RESPONSE_LIMIT {
            return Err(HostError::ResponseBudget(RESPONSE_LIMIT));
        }
        let envelope: Envelope<Capture> = serde_json::from_str(text).map_err(invalid)?;
        envelope.check()?;
        let Capture { stamp, snapshot } = envelope.data;
        let digest = format!("{:x}", Sha256::digest(snapshot.get().as_bytes()));
        if digest != stamp.snapshot_sha256 {
            return Err(invalid("snapshot digest mismatch"));
        }
        let value = super::exact::project(snapshot.get())?;
        if !value.is_object() {
            return Err(invalid("snapshot must be an object"));
        }
        Ok(Self {
            raw: snapshot,
            stamp,
            value,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    version: u32,
    ok: bool,
    data: T,
}

impl<T> Envelope<T> {
    fn check(&self) -> Result<(), HostError> {
        if self.version != 1 || !self.ok {
            return Err(invalid("invalid snapshot success envelope"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    stamp: SnapshotStamp,
    snapshot: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Validation {
    valid: bool,
    current: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Release {
    released: bool,
}

fn invalid(message: impl std::fmt::Display) -> HostError {
    HostError::Request(format!("snapshot codec: {message}"))
}

impl CompatibilityHost {
    fn require_snapshots(&self, world: u64) -> Result<(), HostError> {
        if !self.worlds.contains(&world) || !self.strict.contains_key(&world) {
            return Err(invalid("requires an owned strict world"));
        }
        if !self.negotiation.native["capabilities"]
            .as_array()
            .is_some_and(|a| a.contains(&json!("snapshot_primitives_v1")))
        {
            return Err(invalid("native snapshot_primitives_v1 unavailable"));
        }
        Ok(())
    }

    /// Captures the initialized Idle world using private native host authority.
    pub fn capture_snapshot(&self, world: u64) -> Result<super::CapturedSnapshot, HostError> {
        self.require_snapshots(world)?;
        let response = self.control_text(world, json!({"op":"snapshot.capture"}))?;
        let snapshot = CapturedSnapshot::parse(&response)?;
        if snapshot.stamp.world != world {
            return Err(invalid("wrong snapshot world"));
        }
        Ok(snapshot)
    }

    /// Returns whether the exact retained receipt is current. False is allowed
    /// only for a historical validation; stale current requests fail natively.
    pub fn validate_snapshot(
        &self,
        world: u64,
        snapshot: &CapturedSnapshot,
        require_current: bool,
    ) -> Result<bool, HostError> {
        self.require_snapshots(world)?;
        let text = self.control_text(world, json!({"op":"snapshot.validate","stamp":snapshot.stamp,"require_current":require_current}))?;
        let envelope: Envelope<Validation> = serde_json::from_str(&text).map_err(invalid)?;
        envelope.check()?;
        if !envelope.data.valid || (require_current && !envelope.data.current) {
            return Err(invalid("invalid native validation"));
        }
        Ok(envelope.data.current)
    }

    pub fn release_snapshot(
        &self,
        world: u64,
        snapshot: &CapturedSnapshot,
    ) -> Result<(), HostError> {
        self.require_snapshots(world)?;
        let text = self.control_text(
            world,
            json!({"op":"snapshot.release","stamp":snapshot.stamp}),
        )?;
        let envelope: Envelope<Release> = serde_json::from_str(&text).map_err(invalid)?;
        envelope.check()?;
        if !envelope.data.released {
            return Err(invalid("native release was not acknowledged"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
