use super::{State, pending, record};
use crate::{
    DisunityWorld,
    readiness::{Outcome, tickets},
    strict::Phase,
};
use serde_json::{Value, json};
pub(super) fn enter(w: &mut DisunityWorld, id: u64, index: usize) -> Result<Value, String> {
    let b = pending(w, id)?;
    if b.state != State::Accepted || b.cursor != index || b.entered.is_some() {
        return Err("callback entry requires accepted next index".into());
    }
    let expected = b
        .envelope
        .callbacks
        .get(index)
        .ok_or("callback index outside batch")?
        .clone();
    tickets::reselect(w, Phase::Cleanup)?;
    let selected = w.readiness.ticket.as_ref().and_then(|t| {
        if let tickets::Kind::Callback(k) = t.kind {
            Some(record(t.component, k))
        } else {
            None
        }
    });
    if selected.as_ref() != Some(&expected) {
        w.readiness.batch.as_mut().unwrap().state = State::Cut;
        return Ok(json!({"execute":false,"replan":true}));
    }
    let t = w.readiness.ticket.as_ref().unwrap();
    let decision =
        json!({"ticket":t.id,"component":t.component,"kind":expected.kind,"routine":null});
    let ticket = t.id;
    let entered = tickets::enter(w, Phase::Cleanup, ticket)?;
    if entered["execute"] != true {
        return Err("native batch selection became ineligible".into());
    }
    w.readiness.batch.as_mut().unwrap().entered = Some(ticket);
    Ok(json!({"execute":true,"replan":false,"decision":decision}))
}
pub(crate) fn completion_allowed(
    w: &DisunityWorld,
    ticket: u64,
    outcome: Outcome,
) -> Result<(), String> {
    if w.readiness.completed == Some((ticket, outcome)) {
        return Ok(());
    }
    if !w
        .readiness
        .batch
        .as_ref()
        .is_some_and(|b| b.entered == Some(ticket))
    {
        return Err("Cleanup completion requires entered batch ticket".into());
    }
    Ok(())
}
pub(crate) fn complete(w: &mut DisunityWorld, ticket: u64) {
    if let Some(b) = &mut w.readiness.batch
        && b.entered == Some(ticket)
    {
        b.entered = None;
        b.cursor += 1;
    }
}
