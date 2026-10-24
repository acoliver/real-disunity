pub(crate) mod batches;
mod callbacks;
mod nested;
pub(crate) const NESTED_PROFILE: &str = "native_readiness_nested_start_v1";
pub(crate) const MAX_CHILD_DEPTH: usize = 32;
mod routines;
mod tickets;
mod waits;
use crate::{
    DisunityWorld,
    strict::{self, Context, Phase},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use waits::Wait;

pub(crate) const LIMIT: usize = 4096;
#[derive(Clone, Default)]
pub(crate) struct NativeReadinessState {
    scripts: BTreeMap<u64, callbacks::Script>,
    routines: BTreeMap<u64, routines::Routine>,
    root: u64,
    claims: usize,
    ticket: Option<tickets::Ticket>,
    children: Vec<nested::Scope>,
    completed: Option<(u64, Outcome)>,
    batch: Option<batches::Pending>,
    batch_issues: usize,
}
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum Callback {
    Awake,
    OnEnable,
    Start,
    FixedUpdate,
    Update,
    LateUpdate,
    OnDisable,
    OnDestroy,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Ok,
    Failed,
    More,
    Done,
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum ReadinessRequest {
    #[serde(rename = "readiness.register")]
    Register {
        context: Context,
        component: u64,
        callbacks: Vec<Callback>,
    },
    #[serde(rename = "readiness.next")]
    Next { context: Context },
    #[serde(rename = "readiness.enter")]
    Enter { context: Context, ticket: u64 },
    #[serde(rename = "readiness.complete")]
    Complete {
        context: Context,
        ticket: u64,
        outcome: Outcome,
    },
    #[serde(rename = "readiness.start")]
    Start { context: Context, component: u64 },
    #[serde(rename = "readiness.suspend")]
    Suspend {
        context: Context,
        routine: u64,
        wait: Option<Wait>,
    },
    #[serde(rename = "readiness.cancel")]
    Cancel { context: Context, routine: u64 },
    #[serde(rename = "readiness.release")]
    Release { context: Context, routine: u64 },
}
impl ReadinessRequest {
    pub(crate) fn execute(self, w: &mut DisunityWorld, world: u64) -> Result<Value, String> {
        let context = self.context();
        strict::authorize(w, world, context)?;
        let phase = context.phase();
        let root = context.sequence();
        profile_guard(w, &self, phase)?;
        reset_root(w, root);
        match self {
            Self::Register { .. } if strict::is_terminal(w) => {
                Err("terminal Cleanup forbids readiness registration".into())
            }
            Self::Register {
                component,
                callbacks,
                ..
            } => {
                let result = callbacks::register(w, component, callbacks)?;
                Ok(if strict::batch_profile(w) {
                    batches::binding(component)
                } else {
                    result
                })
            }
            Self::Next { .. } => tickets::next(w, phase),
            Self::Enter { ticket, .. } => tickets::enter(w, phase, ticket),
            Self::Complete {
                ticket, outcome, ..
            } => {
                let value = tickets::complete(w, ticket, outcome)?;
                batches::complete(w, ticket);
                Ok(value)
            }
            Self::Start { component, .. } => routines::start(w, component, phase),
            Self::Suspend { routine, wait, .. } => waits::suspend(w, routine, wait, phase),
            Self::Cancel { routine, .. } => routines::cancel(w, routine),
            Self::Release { routine, .. } => routines::release(w, routine),
        }
    }
    fn context(&self) -> &Context {
        match self {
            Self::Register { context, .. }
            | Self::Next { context }
            | Self::Enter { context, .. }
            | Self::Complete { context, .. }
            | Self::Start { context, .. }
            | Self::Suspend { context, .. }
            | Self::Cancel { context, .. }
            | Self::Release { context, .. } => context,
        }
    }
}
pub(crate) fn require_drained(w: &DisunityWorld, phase: Phase) -> Result<(), String> {
    if w.readiness.batch.is_some()
        || w.readiness.ticket.is_some()
        || !w.readiness.children.is_empty()
        || w.readiness.routines.values().any(|r| r.state.is_inflight())
    {
        return Err("readiness operation must complete and suspend before phase.finish".into());
    }
    if phase == Phase::Cleanup
        && (callbacks::cleanup_pending(w)
            || w.readiness
                .routines
                .values()
                .any(|r| !matches!(r.state, routines::State::Terminal)))
    {
        if strict::is_terminal(w) && w.readiness.claims >= LIMIT {
            return Err("cleanup_budget_exhausted: native cleanup remains incomplete".into());
        }
        return Err("Cleanup requires all lifecycle and routine disposal attempts".into());
    }
    Ok(())
}
pub(crate) fn observe(w: &mut DisunityWorld) {
    if !w.readiness.routines.is_empty() {
        routines::refresh(w, Phase::Initialize);
    }
}
fn spend(w: &mut DisunityWorld) -> Result<(), String> {
    if w.readiness.claims >= LIMIT {
        return Err("readiness root claim budget exceeded".into());
    }
    w.readiness.claims += 1;
    Ok(())
}
fn live_script(w: &DisunityWorld, component: u64) -> Result<&crate::components::Component, String> {
    let c = w.components[w.component_index(component)?]
        .as_ref()
        .unwrap();
    if c.kind != "Script" {
        return Err("readiness owner must be a live Script component".into());
    }
    Ok(c)
}
fn eligibility(w: &DisunityWorld, component: u64) -> (bool, bool, bool) {
    let Ok(c) = live_script(w, component) else {
        return (false, false, true);
    };
    let dying = w.pending_script(component, c.object);
    (w.active(c.object), c.enabled, dying)
}
fn empty() -> Value {
    json!({})
}

pub(crate) fn begin_cleanup(w: &mut DisunityWorld, root: u64) -> u32 {
    w.readiness.batch = None;
    w.readiness.batch_issues = 0;
    let unknown = tickets::abandon(w);
    routines::refresh(w, Phase::Cleanup);
    w.readiness.root = root;
    w.readiness.claims = 0;
    w.readiness.completed = None;
    unknown
}

pub(crate) fn cleanup_status(w: &DisunityWorld) -> Value {
    json!({"remaining_scripts":callbacks::cleanup_count(w),
        "remaining_routines":w.readiness.routines.values()
            .filter(|r| !matches!(r.state,routines::State::Terminal)).count(),
        "claims":w.readiness.claims,"max_claims":LIMIT,
        "attempts_drained":require_drained(w,Phase::Cleanup).is_ok(),
        "outstanding_ticket":w.readiness.ticket.is_some() || !w.readiness.children.is_empty()})
}

fn reset_root(w: &mut DisunityWorld, root: u64) {
    if w.readiness.root != root {
        w.readiness.root = root;
        w.readiness.claims = 0;
        w.readiness.completed = None;
        w.readiness.batch_issues = 0;
    }
}
fn profile_guard(w: &DisunityWorld, r: &ReadinessRequest, phase: Phase) -> Result<(), String> {
    if !strict::batch_profile(w) {
        return Ok(());
    }
    match r {
        ReadinessRequest::Register { callbacks, .. } => {
            if phase != Phase::Initialize
                || callbacks.iter().any(|cb| {
                    !matches!(
                        cb,
                        Callback::Awake
                            | Callback::OnEnable
                            | Callback::OnDisable
                            | Callback::OnDestroy
                    )
                })
            {
                return Err(
                    "Cleanup batch profile only registers Initialize lifecycle metadata".into(),
                );
            }
        }
        ReadinessRequest::Start { .. }
        | ReadinessRequest::Suspend { .. }
        | ReadinessRequest::Cancel { .. }
        | ReadinessRequest::Release { .. } => {
            return Err("routines unsupported by Cleanup batch profile".into());
        }
        ReadinessRequest::Next { .. } | ReadinessRequest::Enter { .. }
            if phase == Phase::Cleanup =>
        {
            return Err("Cleanup requires callbacks batch entry".into());
        }
        ReadinessRequest::Complete {
            ticket, outcome, ..
        } if phase == Phase::Cleanup => {
            batches::completion_allowed(w, *ticket, *outcome)?;
        }
        _ => {}
    }
    Ok(())
}
