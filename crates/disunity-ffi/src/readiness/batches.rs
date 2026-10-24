mod entry;
mod exact;
pub(crate) mod raw;
mod wire;
use super::{LIMIT, callbacks, tickets};
use crate::{
    DisunityWorld,
    strict::{self, Context, Phase},
};
use exact::Exact;
use serde::Deserialize;
use serde_json::{Value, json};
use wire::{Envelope, Payload, Projection, Record};
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Reserved,
    Accepted,
    Cut,
}
#[derive(Clone)]
pub(super) struct Pending {
    envelope: Envelope,
    state: State,
    cursor: usize,
    entered: Option<u64>,
}
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum Request {
    #[serde(rename = "callbacks.next")]
    Next { context: Context },
    #[serde(rename = "callbacks.inspect")]
    Inspect { context: Context, batch: Exact },
    #[serde(rename = "callbacks.accept")]
    Accept {
        context: Context,
        envelope: Envelope,
    },
    #[serde(rename = "callbacks.enter")]
    Enter {
        context: Context,
        batch: Exact,
        index: usize,
    },
    #[serde(rename = "callbacks.finish")]
    Finish { context: Context, batch: Exact },
}
impl Request {
    pub(crate) fn changes_content(&self) -> bool {
        !matches!(self, Self::Inspect { .. })
    }
    pub(crate) fn execute(self, w: &mut DisunityWorld, world: u64) -> Result<Value, String> {
        let c = match &self {
            Self::Next { context }
            | Self::Inspect { context, .. }
            | Self::Accept { context, .. }
            | Self::Enter { context, .. }
            | Self::Finish { context, .. } => context,
        };
        strict::authorize(w, world, c)?;
        if !strict::batch_profile(w) || c.phase() != Phase::Cleanup {
            return Err("callbacks operations require native Cleanup batch profile".into());
        }
        super::reset_root(w, c.sequence());
        match self {
            Self::Next { context } => next(w, &context),
            Self::Inspect { batch, .. } => inspect(w, batch.0),
            Self::Accept { envelope, .. } => accept(w, envelope),
            Self::Enter { batch, index, .. } => entry::enter(w, batch.0, index),
            Self::Finish { batch, .. } => finish(w, batch.0),
        }
    }
}
fn pending(w: &DisunityWorld, id: u64) -> Result<&Pending, String> {
    w.readiness
        .batch
        .as_ref()
        .filter(|b| b.envelope.context.callback_batch.0 == id)
        .ok_or_else(|| "unknown or stale callback batch".into())
}
fn record(component: u64, kind: super::Callback) -> Record {
    Record {
        component: Exact(component),
        binding: Exact(component),
        kind,
        payload: Payload {},
    }
}
fn next(w: &mut DisunityWorld, c: &Context) -> Result<Value, String> {
    if w.readiness.batch.is_some() {
        return Err("callback batch is nonreentrant".into());
    }
    if w.readiness.batch_issues >= LIMIT {
        return Err("callback issuance budget exceeded".into());
    }
    let records = if let Some(t) = &w.readiness.ticket {
        let tickets::Kind::Callback(kind) = t.kind else {
            return Err("unsupported batch ticket".into());
        };
        if t.entered {
            return Err("callback ticket is entered".into());
        }
        vec![record(t.component, kind)]
    } else {
        callbacks::reservation(w)
            .into_iter()
            .map(|(c, k)| record(c, k))
            .collect()
    };
    if records.is_empty() {
        tickets::next(w, Phase::Cleanup)?;
        return Ok(Value::Null);
    }
    let id = u64::from(crate::next_generation()) << 32;
    let envelope = Envelope {
        version: 1,
        context: Projection::new(w, c, id)?,
        callbacks: records,
    };
    let value = serde_json::to_value(&envelope).map_err(|_| "batch encoding failed")?;
    if serde_json::to_vec(&value)
        .map_err(|_| "batch encoding failed")?
        .len()
        > crate::response::MAX_REQUEST_BYTES
    {
        return Err("callback envelope byte budget exceeded".into());
    }
    w.readiness.batch = Some(Pending {
        envelope,
        state: State::Reserved,
        cursor: 0,
        entered: None,
    });
    w.readiness.batch_issues += 1;
    Ok(value)
}
fn inspect(w: &DisunityWorld, id: u64) -> Result<Value, String> {
    let b = pending(w, id)?;
    Ok(
        json!({"envelope":b.envelope,"cursor":b.cursor,"accepted":b.state == State::Accepted,"cut":b.state == State::Cut}),
    )
}
fn accept(w: &mut DisunityWorld, envelope: Envelope) -> Result<Value, String> {
    let b = pending(w, envelope.context.callback_batch.0)?;
    if b.state != State::Reserved || b.envelope != envelope {
        return Err("callback envelope differs from native reservation or was accepted".into());
    }
    w.readiness.batch.as_mut().unwrap().state = State::Accepted;
    Ok(json!({"accepted":true}))
}
fn finish(w: &mut DisunityWorld, id: u64) -> Result<Value, String> {
    let b = pending(w, id)?;
    if b.entered.is_some()
        || (b.state != State::Cut
            && (b.state != State::Accepted || b.cursor != b.envelope.callbacks.len()))
    {
        return Err("callback batch is not drained".into());
    }
    w.readiness.batch = None;
    Ok(json!({}))
}
pub(super) use entry::{complete, completion_allowed};

pub(super) fn binding(component: u64) -> Value {
    json!({"binding":Exact(component)})
}
