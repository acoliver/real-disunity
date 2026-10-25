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
    child: Child,
}

#[derive(Clone)]
enum Child {
    Routine(u64),
    Lifecycle,
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
        return Err("immediate child requires an entered lifecycle callback or MoveNext".into());
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
    w.readiness.children.push(Scope {
        parent,
        child: Child::Routine(child),
    });
    tickets::immediate(w, child)
}

pub(super) fn active(w: &DisunityWorld) -> Option<u64> {
    match w.readiness.children.last()?.child {
        Child::Routine(id) => Some(id),
        Child::Lifecycle => None,
    }
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
    let Some(scope) = w.readiness.children.last() else {
        return;
    };
    let finished = match scope.child {
        Child::Lifecycle => true,
        Child::Routine(id) => matches!(
            w.readiness.routines[&id].state,
            State::Waiting(_) | State::Terminal
        ),
    };
    if w.readiness.ticket.is_none() && finished {
        w.readiness.ticket = Some(w.readiness.children.pop().unwrap().parent);
    }
}

pub(super) fn registration_parent(
    w: &DisunityWorld,
    component: u64,
) -> Result<Option<u64>, String> {
    let Some(parent) = w.readiness.ticket.as_ref() else {
        return Ok(None);
    };
    if !crate::strict::nested_readiness(w)
        || !parent.entered
        || !matches!(parent.kind, Kind::Callback(_) | Kind::MoveNext)
        || !super::eligibility(w, component).0
    {
        return Ok(None);
    }
    admit(w)?;
    Ok(Some(parent.id))
}

pub(super) fn lifecycle(w: &mut DisunityWorld, phase: super::Phase) -> Result<Value, String> {
    let parent = w.readiness.ticket.as_ref().unwrap();
    if !crate::strict::nested_readiness(w) || !parent.entered {
        return Err("readiness ticket is nonreentrant".into());
    }
    let Some((component, callback)) = super::callbacks::immediate(w, phase, parent.id)? else {
        return Ok(Value::Null);
    };
    admit(w)?;
    let parent = w.readiness.ticket.take().unwrap();
    w.readiness.children.push(Scope {
        parent,
        child: Child::Lifecycle,
    });
    tickets::issue(w, component, None, Kind::Callback(callback))
}
