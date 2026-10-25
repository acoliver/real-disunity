use super::{
    Callback, Outcome, Phase, callbacks,
    routines::{self, State},
    spend, waits,
};
use crate::DisunityWorld;
use serde::Serialize;
use serde_json::{Value, json};
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
pub(super) enum Kind {
    Callback(Callback),
    MoveNext,
    Current,
    Dispose,
}
#[derive(Clone)]
pub(super) struct Ticket {
    pub(super) id: u64,
    pub(super) component: u64,
    pub(super) routine: Option<u64>,
    pub(super) kind: Kind,
    pub(super) entered: bool,
}
pub(super) fn next(w: &mut DisunityWorld, phase: Phase) -> Result<Value, String> {
    if w.readiness.ticket.is_some() {
        return super::nested::lifecycle(w, phase);
    }
    routines::refresh(w, phase);
    if let Some(id) = super::nested::active(w) {
        return immediate(w, id);
    }
    match select(w, phase)? {
        Some(selection) => issue(w, selection.component, selection.routine, selection.kind),
        None => Ok(Value::Null),
    }
}
#[derive(PartialEq, Eq)]
struct TicketSelection {
    component: u64,
    routine: Option<u64>,
    kind: Kind,
}
fn select(w: &mut DisunityWorld, phase: Phase) -> Result<Option<TicketSelection>, String> {
    if let Some((component, callback)) = callbacks::select(w, phase, 0..2)? {
        return Ok(Some(TicketSelection {
            component,
            routine: None,
            kind: Kind::Callback(callback),
        }));
    }
    let disposal = w
        .readiness
        .routines
        .iter()
        .find_map(|(id, r)| matches!(r.state, State::Disposing).then_some((*id, r.owner)));
    if let Some((id, owner)) = disposal {
        return Ok(Some(TicketSelection {
            component: owner,
            routine: Some(id),
            kind: Kind::Dispose,
        }));
    }
    if let Some((component, callback)) = callbacks::select(w, phase, 2..5)? {
        return Ok(Some(TicketSelection {
            component,
            routine: None,
            kind: Kind::Callback(callback),
        }));
    }
    let next = w.readiness.routines.iter().find_map(|(id, r)| {
        let kind = match &r.state {
            State::Ready => Kind::MoveNext,
            State::Current => Kind::Current,
            State::Disposing => Kind::Dispose,
            State::Waiting(wait) if waits::ready(w, wait, phase) => Kind::MoveNext,
            _ => return None,
        };
        Some((*id, r.owner, kind))
    });
    Ok(next.map(|(id, owner, kind)| TicketSelection {
        component: owner,
        routine: Some(id),
        kind,
    }))
}
pub(super) fn issue(
    w: &mut DisunityWorld,
    component: u64,
    routine: Option<u64>,
    kind: Kind,
) -> Result<Value, String> {
    if w.readiness.claims >= super::LIMIT {
        return Err("readiness root claim budget exceeded".into());
    }
    let id = u64::from(crate::next_generation()) << 32;
    let name = match kind {
        Kind::Callback(cb) => json!(cb),
        _ => json!(kind),
    };
    w.readiness.ticket = Some(Ticket {
        id,
        component,
        routine,
        kind,
        entered: false,
    });
    Ok(json!({"ticket":id,"kind":name,"component":component,"routine":routine}))
}
pub(super) fn enter(w: &mut DisunityWorld, phase: Phase, id: u64) -> Result<Value, String> {
    let t = w.readiness.ticket.clone().ok_or("no readiness ticket")?;
    if t.id != id || t.entered {
        return Err("readiness ticket is stale or already entered".into());
    }
    routines::refresh(w, phase);
    let eligible = match t.kind {
        Kind::Callback(cb) => callbacks::eligible(w, t.component, cb, phase),
        Kind::Dispose => matches!(
            w.readiness.routines[&t.routine.unwrap()].state,
            State::Disposing
        ),
        _ => !matches!(
            w.readiness.routines[&t.routine.unwrap()].state,
            State::Disposing | State::Terminal
        ),
    };
    if !eligible {
        w.readiness.ticket = None;
        super::nested::restore(w);
        return Ok(json!({"execute":false}));
    }
    spend(w)?;
    if let Kind::Callback(cb) = t.kind {
        callbacks::consume(w, t.component, cb);
    }
    w.readiness.ticket.as_mut().unwrap().entered = true;
    Ok(json!({"execute":true}))
}
pub(super) fn complete(w: &mut DisunityWorld, id: u64, outcome: Outcome) -> Result<Value, String> {
    if w.readiness.completed == Some((id, outcome)) {
        return Ok(super::empty());
    }
    let t = w.readiness.ticket.as_ref().ok_or("no readiness ticket")?;
    if t.id != id || !t.entered {
        return Err("completion requires the entered current ticket".into());
    }
    let kind = t.kind;
    if let Some(id) = t.routine {
        let r = w.readiness.routines.get_mut(&id).unwrap();
        let state = match (kind, outcome) {
            (Kind::MoveNext, Outcome::More) => State::Current,
            (Kind::MoveNext, Outcome::Done | Outcome::Failed)
            | (Kind::Current, Outcome::Failed) => State::Disposing,
            (Kind::Current, Outcome::Ok) => State::Suspend,
            (Kind::Dispose, Outcome::Ok | Outcome::Failed) => State::Terminal,
            _ => return Err("invalid iterator outcome".into()),
        };
        if !matches!(r.state, State::Disposing) || kind == Kind::Dispose {
            r.state = state;
        }
    } else if !matches!(outcome, Outcome::Ok | Outcome::Failed) {
        return Err("invalid callback outcome".into());
    }
    w.readiness.ticket = None;
    w.readiness.completed = Some((id, outcome));
    super::nested::restore(w);
    Ok(super::empty())
}

pub(super) fn abandon(w: &mut DisunityWorld) -> u32 {
    let mut tickets: Vec<Ticket> = w.readiness.ticket.take().into_iter().collect();
    tickets.extend(w.readiness.children.drain(..).map(|s| s.parent));
    let mut unknown = 0;
    for ticket in tickets {
        if ticket.entered && ticket.kind == Kind::Dispose {
            w.readiness
                .routines
                .get_mut(&ticket.routine.unwrap())
                .unwrap()
                .state = State::Terminal;
        }
        unknown += u32::from(ticket.entered);
    }
    unknown
}

pub(super) fn immediate(w: &mut DisunityWorld, id: u64) -> Result<Value, String> {
    let r = &w.readiness.routines[&id];
    let kind = match r.state {
        State::Ready => Kind::MoveNext,
        State::Current => Kind::Current,
        State::Disposing => Kind::Dispose,
        _ => return Err("immediate child requires suspension or disposal".into()),
    };
    issue(w, r.owner, Some(id), kind)
}

pub(super) fn reselect(w: &mut DisunityWorld, phase: Phase) -> Result<(), String> {
    let Some(t) = &w.readiness.ticket else {
        return next(w, phase).map(|_| ());
    };
    if t.entered || !w.readiness.children.is_empty() {
        return Err("batch selection requires one unentered root ticket".into());
    }
    let previous = TicketSelection {
        component: t.component,
        routine: t.routine,
        kind: t.kind,
    };
    routines::refresh(w, phase);
    let choice = select(w, phase)?;
    if choice.as_ref() == Some(&previous) {
        return Ok(());
    }
    w.readiness.ticket = None;
    if let Some(selection) = choice {
        issue(w, selection.component, selection.routine, selection.kind)?;
    }
    Ok(())
}
