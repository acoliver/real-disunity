use super::{
    LIMIT, MAX_CHILD_DEPTH,
    routines::State,
    tickets::{self, Kind, Ticket},
};
use crate::DisunityWorld;
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Scope {
    pub parent: Ticket,
    child: u64,
}

pub(super) fn admit(w: &DisunityWorld) -> Result<bool, String> {
    if !crate::strict::nested_readiness(w) {
        return Ok(false);
    }
    let Some(parent) = &w.readiness.ticket else {
        if !w.readiness.children.is_empty() {
            return Err("immediate child pipeline has no entered callback".into());
        }
        return Ok(false);
    };
    if !parent.entered || !matches!(parent.kind, Kind::Callback(_) | Kind::MoveNext) {
        return Err("nested start requires an entered lifecycle callback or MoveNext".into());
    }
    if w.readiness.children.len() >= MAX_CHILD_DEPTH {
        return Err("immediate child depth budget exceeded".into());
    }
    // Leave three claims for each child and its suspended native ancestors.
    if w.readiness.claims + 3 * (w.readiness.children.len() + 1) > LIMIT {
        return Err("immediate child root claim budget exceeded".into());
    }
    Ok(true)
}

pub(super) fn start(w: &mut DisunityWorld, child: u64) -> Result<Value, String> {
    let parent = w.readiness.ticket.take().unwrap();
    w.readiness.children.push(Scope { parent, child });
    tickets::immediate(w, child)
}

pub(super) fn active(w: &DisunityWorld) -> Option<u64> {
    w.readiness.children.last().map(|s| s.child)
}

pub(super) fn ancestor(w: &DisunityWorld, routine: u64) -> bool {
    w.readiness
        .children
        .iter()
        .any(|s| s.parent.routine == Some(routine))
}

pub(super) fn suspending(w: &DisunityWorld, routine: u64) -> Result<(), String> {
    if crate::strict::nested_readiness(w)
        && (active(w).is_some_and(|id| id != routine) || w.readiness.ticket.is_some())
    {
        return Err("suspend requires the current child without an outstanding ticket".into());
    }
    Ok(())
}

pub(super) fn restore(w: &mut DisunityWorld) {
    let Some(child) = active(w) else {
        return;
    };
    if w.readiness.ticket.is_none()
        && matches!(
            w.readiness.routines[&child].state,
            State::Waiting(_) | State::Terminal
        )
    {
        w.readiness.ticket = Some(w.readiness.children.pop().unwrap().parent);
    }
}
