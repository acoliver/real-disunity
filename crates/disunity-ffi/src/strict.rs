mod control;
mod idle_mutation;
pub(crate) use idle_mutation::{IDLE_PROFILE, IdleRename, mutate_idle};
mod lease;
mod schedule;
mod terminal;
use crate::{DisunityWorld, input::Input};
pub(crate) use schedule::phases as supported_phases;
pub(crate) use schedule::{Phase, Stage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub(crate) const PROFILE: &str = "native_phase_context_v1";
pub(crate) const MAX_CLAIMS: u32 = 4096;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Host {
    session: String,
    host_key: String,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Context {
    pub(crate) world: u64,
    pub(crate) session: String,
    pub(crate) frame: u64,
    pub(crate) fixed_tick: u64,
    pub(crate) phase: Phase,
    pub(crate) sequence: u64,
    pub(crate) token: String,
}
#[derive(Clone)]
pub(crate) struct Lease {
    context: Context,
    claimed: bool,
    clock_before: crate::clock::Clock,
    stage_before: Stage,
}
#[derive(Clone)]
pub(crate) struct Authority {
    world: u64,
    profile: &'static str,
    session: String,
    host_key: String,
    pub(crate) stage: Stage,
    pub(crate) active: Option<Lease>,
    sequence: u64,
    revision: u64,
    claims: u32,
    remaining: u32,
    pending_input: Option<Input>,
    pending_clock: Option<crate::clock::Clock>,
    terminal: Option<terminal::Recovery>,
}
impl Authority {
    fn authenticate(&self, host: &Host) -> Result<(), String> {
        if host.session != self.session || host.host_key != self.host_key {
            return Err("invalid host credential".into());
        }
        if self.stage == Stage::Closed {
            return Err("strict session is closed".into());
        }
        Ok(())
    }
    fn idle_lease(&self) -> Result<(), String> {
        if self.active.is_some() {
            return Err("phase lease is nonreentrant".into());
        }
        Ok(())
    }
    fn bump(&mut self) -> Result<(), String> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("schedule revision overflow")?;
        Ok(())
    }
}
pub(crate) fn nested_readiness(w: &DisunityWorld) -> bool {
    w.strict
        .as_ref()
        .is_some_and(|a| a.profile == crate::readiness::NESTED_PROFILE)
}
pub(crate) fn entropy() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "OS credential entropy unavailable")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn allow_mutation(w: &DisunityWorld) -> Result<(), String> {
    if let Some(a) = &w.strict {
        if a.active.as_ref().is_some_and(|l| l.claimed) {
            return Ok(());
        }
        if a.stage != Stage::Phase(Phase::Initialize) || a.active.is_some() {
            return Err("gameplay mutation requires a claimed phase root".into());
        }
    }
    Ok(())
}
pub(crate) fn status(w: &DisunityWorld) -> Value {
    let Some(a) = &w.strict else {
        return json!({"profile":"legacy","content_revision":w.content_revision,"snapshot_receipts":w.snapshots.count(),"snapshot_bytes":w.snapshots.bytes()});
    };
    json!({"profile":a.profile,"world":a.world,"world_generation":a.world >> 32,"session":a.session,
        "frame":w.clock.frame,"fixed_tick":w.clock.fixed_frame,
        "pending_fixed_tick":a.pending_input.as_ref().map(|_| w.clock.fixed_frame + 1),
        "stage":a.stage,"content_revision":w.content_revision,
        "snapshot_receipts":w.snapshots.count(),"snapshot_bytes":w.snapshots.bytes(),"revision":a.revision,"revision_kind":"schedule_transition",
        "sequence":a.sequence,"claims":a.claims,"remaining_fixed":a.remaining,
        "terminal_cleanup":a.terminal.is_some(),
        "terminal_recovery":a.terminal.as_ref().map(|r| json!({"claims":r.claims,
            "abandoned_fixed_tick":r.abandoned_tick,"execution_unknown":r.execution_unknown,
            "native_obligations":crate::readiness::cleanup_status(w)})),
        "active_phase":a.active.as_ref().map(|l| json!({"phase":l.context.phase,
            "sequence":l.context.sequence,"claimed":l.claimed}))})
}
pub(crate) use control::{abort, begin, close, commit, configure, end, open};
pub(crate) use lease::{cancel, claim, finish, issue};

impl Context {
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }
}
pub(crate) fn authorize(w: &DisunityWorld, world: u64, context: &Context) -> Result<(), String> {
    let a = w
        .strict
        .as_ref()
        .ok_or("readiness requires strict phase authority")?;
    let l = a
        .active
        .as_ref()
        .ok_or("readiness requires an active phase")?;
    if a.world != world || context.world != world || l.context != *context || !l.claimed {
        return Err("readiness requires the exact claimed private context".into());
    }
    Ok(())
}

pub(crate) fn snapshot_identity(
    w: &DisunityWorld,
    host: &Host,
    world: u64,
) -> Result<(String, u64), String> {
    let a = w
        .strict
        .as_ref()
        .ok_or("snapshot requires strict session")?;
    a.authenticate(host)?;
    if a.world != world {
        return Err("snapshot world mismatch".into());
    }
    Ok((a.session.clone(), a.revision))
}
pub(crate) fn snapshot_idle(w: &DisunityWorld) -> Result<(), String> {
    let a = w
        .strict
        .as_ref()
        .ok_or("snapshot requires strict session")?;
    if a.stage != Stage::Idle || a.active.is_some() {
        return Err("snapshot capture requires initialized Idle".into());
    }
    crate::readiness::require_drained(w, Phase::Initialize)
}

pub(crate) use terminal::{begin_cleanup, is_terminal};

pub(crate) use terminal::CleanupReason;
