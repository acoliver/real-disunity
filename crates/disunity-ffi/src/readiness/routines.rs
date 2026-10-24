use super::{LIMIT, Phase, Wait, eligibility};
use crate::DisunityWorld;
use serde_json::{Value, json};
#[derive(Clone)]
pub(super) struct Routine {
    pub owner: u64,
    pub state: State,
}
#[derive(Clone)]
pub(super) enum State {
    Ready,
    Current,
    Suspend,
    Waiting(Wait),
    Disposing,
    Terminal,
}
impl State {
    pub(super) fn is_inflight(&self) -> bool {
        matches!(self, Self::Current | Self::Suspend)
    }
}
pub(super) fn start(w: &mut DisunityWorld, component: u64, phase: Phase) -> Result<Value, String> {
    super::live_script(w, component)?;
    if !w.readiness.scripts.contains_key(&component) {
        return Err("routine owner is not registered".into());
    }
    let (active, _, dying) = eligibility(w, component);
    if !active || dying || phase == Phase::Cleanup {
        return Err("routine owner is inactive or terminal".into());
    }
    if w.readiness.routines.len() >= LIMIT {
        return Err("routine readiness budget exceeded".into());
    }
    let immediate = super::nested::admit(w)?;
    let id = u64::from(crate::next_generation()) << 32;
    w.readiness.routines.insert(
        id,
        Routine {
            owner: component,
            state: State::Ready,
        },
    );
    if immediate {
        let decision = super::nested::start(w, id)?;
        Ok(json!({"routine":id,"immediate":decision}))
    } else {
        Ok(json!({"routine":id}))
    }
}
pub(super) fn cancel(w: &mut DisunityWorld, id: u64) -> Result<Value, String> {
    let r = w
        .readiness
        .routines
        .get_mut(&id)
        .ok_or("unknown or stale routine")?;
    if !matches!(r.state, State::Terminal) {
        r.state = State::Disposing;
    }
    Ok(super::empty())
}
pub(super) fn release(w: &mut DisunityWorld, id: u64) -> Result<Value, String> {
    let r = w
        .readiness
        .routines
        .get(&id)
        .ok_or("unknown or stale routine")?;
    if !matches!(r.state, State::Terminal)
        || w.readiness
            .routines
            .values()
            .any(|r| matches!(r.state,State::Waiting(Wait::Join { routine, .. }) if routine == id))
    {
        return Err("routine release requires terminal state without joiners".into());
    }
    w.readiness.routines.remove(&id);
    Ok(super::empty())
}
pub(super) fn refresh(w: &mut DisunityWorld, phase: Phase) {
    let cancelled: Vec<u64> = w
        .readiness
        .routines
        .iter()
        .filter_map(|(id, r)| {
            let (active, _, dying) = eligibility(w, r.owner);
            ((!active || dying || phase == Phase::Cleanup) && !matches!(r.state, State::Terminal))
                .then_some(*id)
        })
        .collect();
    for id in cancelled {
        w.readiness.routines.get_mut(&id).unwrap().state = State::Disposing;
    }
}
