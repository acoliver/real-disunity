mod digest;
use crate::{
    DisunityWorld,
    strict::{self, Host},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) const MAX_RECEIPTS: usize = 128;
pub(crate) const MAX_SNAPSHOT_BYTES: usize = 32 << 20;

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stamp {
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
#[derive(Clone, Default)]
pub(crate) struct Receipts {
    next: u64,
    bytes: usize,
    retained: BTreeMap<u64, (Stamp, usize)>,
}
impl Receipts {
    pub(crate) fn count(&self) -> usize {
        self.retained.len()
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    fn require(&self, stamp: &Stamp) -> Result<usize, String> {
        self.retained
            .get(&stamp.snapshot_id)
            .filter(|(original, _)| original == stamp)
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| "snapshot stamp is forged, released or not retained".into())
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum SnapshotRequest {
    #[serde(rename = "snapshot.capture")]
    Capture { host: Host },
    #[serde(rename = "snapshot.validate")]
    Validate {
        host: Host,
        stamp: Stamp,
        require_current: bool,
    },
    #[serde(rename = "snapshot.release")]
    Release { host: Host, stamp: Stamp },
}
impl SnapshotRequest {
    pub(crate) fn execute(self, w: &mut DisunityWorld, world: u64) -> Result<Value, String> {
        match self {
            Self::Capture { host } => capture(w, world, &host),
            Self::Validate {
                host,
                stamp,
                require_current,
            } => {
                let current = validate(w, world, &host, &stamp)?;
                if require_current && !current {
                    return Err("snapshot is no longer current".into());
                }
                Ok(json!({"valid":true,"current":current}))
            }
            Self::Release { host, stamp } => {
                validate(w, world, &host, &stamp)?;
                let bytes = w.snapshots.require(&stamp)?;
                w.snapshots.retained.remove(&stamp.snapshot_id);
                w.snapshots.bytes -= bytes;
                Ok(json!({"released":true}))
            }
        }
    }
}
fn capture(w: &mut DisunityWorld, world: u64, host: &Host) -> Result<Value, String> {
    let (session, schedule_revision) = strict::snapshot_identity(w, host, world)?;
    strict::snapshot_idle(w)?;
    if w.snapshots.count() >= MAX_RECEIPTS {
        return Err("snapshot receipt count budget exceeded".into());
    }
    let snapshot = crate::protocol::read::snapshot(w)?;
    let bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SNAPSHOT_BYTES.saturating_sub(w.snapshots.bytes) {
        return Err("snapshot represented byte budget exceeded".into());
    }
    let snapshot_id = w
        .snapshots
        .next
        .checked_add(1)
        .ok_or("snapshot ID overflow")?;
    let stamp = Stamp {
        world,
        world_generation: (world >> 32) as u32,
        session,
        frame: w.clock.frame,
        fixed_tick: w.clock.fixed_frame,
        schedule_revision,
        content_revision: w.content_revision,
        snapshot_id,
        snapshot_sha256: digest::sha256(&bytes),
    };
    w.snapshots.next = snapshot_id;
    w.snapshots.bytes += bytes.len();
    w.snapshots
        .retained
        .insert(snapshot_id, (stamp.clone(), bytes.len()));
    Ok(json!({"stamp":stamp,"snapshot":snapshot}))
}
fn validate(w: &DisunityWorld, world: u64, host: &Host, stamp: &Stamp) -> Result<bool, String> {
    let (session, revision) = strict::snapshot_identity(w, host, world)?;
    if stamp.world != world
        || stamp.world_generation != (world >> 32) as u32
        || stamp.session != session
    {
        return Err("snapshot world or session mismatch".into());
    }
    w.snapshots.require(stamp)?;
    Ok(stamp.content_revision == w.content_revision
        && stamp.schedule_revision == revision
        && stamp.frame == w.clock.frame
        && stamp.fixed_tick == w.clock.fixed_frame)
}
