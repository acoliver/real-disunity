use super::{Context, Host, Lease, MAX_CLAIMS, Phase, Stage, status};
use crate::{DisunityWorld, input::projection};
use serde_json::{Value, json};

pub(crate) fn issue(
    w: &mut DisunityWorld,
    host: &Host,
    phase: Phase,
    identity: (u64, u64),
    token: String,
) -> Result<Value, String> {
    if super::batch_profile(w) && !matches!(phase, Phase::Initialize | Phase::Cleanup) {
        return Err("phase unsupported by Cleanup batch profile".into());
    }
    let a = w.strict.as_ref().ok_or("strict session is not open")?;
    a.authenticate(host)?;
    a.idle_lease()?;
    let tick = if phase.is_fixed() {
        w.clock
            .fixed_frame
            .checked_add(1)
            .ok_or("fixed tick overflow")?
    } else {
        w.clock.fixed_frame
    };
    if !phase.can_issue(a.stage) || identity != (w.clock.frame, tick) || a.claims >= MAX_CLAIMS {
        return Err("phase.open identity, stage or claim budget mismatch".into());
    }
    let clock_before = w.clock.clone();
    if phase == Phase::FixedUpdate {
        let checkpoint = w.input.clone();
        w.strict.as_mut().unwrap().pending_clock = Some(w.clock.clone());
        w.input
            .advance(projection::Phase::Fixed, w.clock.endpoint()?, tick)?;
        w.strict.as_mut().unwrap().pending_input = Some(checkpoint);
    }
    project_clock(w, phase)?;
    let a = w.strict.as_mut().unwrap();
    a.sequence = a.sequence.checked_add(1).ok_or("phase sequence overflow")?;
    let context = Context {
        world: a.world,
        session: a.session.clone(),
        frame: identity.0,
        fixed_tick: tick,
        phase,
        sequence: a.sequence,
        token,
    };
    a.active = Some(Lease {
        context: context.clone(),
        claimed: false,
        clock_before,
        stage_before: a.stage,
    });
    a.stage = Stage::Phase(phase);
    a.bump()?;
    Ok(json!({"context":context,"stamp":status(w)}))
}
pub(crate) fn claim(
    w: &mut DisunityWorld,
    world: u64,
    phase: Phase,
    context: Context,
) -> Result<Value, String> {
    let a = w.strict.as_mut().ok_or("strict session is not open")?;
    let lease = a.active.as_mut().ok_or("phase has no outstanding lease")?;
    if world != context.world
        || world != a.world
        || phase != context.phase
        || lease.context != context
        || lease.claimed
    {
        return Err("phase claim is invalid, stale or already consumed".into());
    }
    lease.claimed = true;
    if let Some(recovery) = &mut a.terminal {
        recovery.claims = recovery
            .claims
            .checked_add(1)
            .filter(|c| *c == 1)
            .ok_or("terminal recovery claim already consumed")?;
    } else {
        a.claims = a
            .claims
            .checked_add(1)
            .filter(|c| *c <= MAX_CLAIMS)
            .ok_or("phase claim budget exceeded")?;
    }
    a.bump()?;
    Ok(json!({"execute":true,"stamp":status(w)}))
}
pub(crate) fn finish(
    w: &mut DisunityWorld,
    host: &Host,
    context: Context,
) -> Result<Value, String> {
    crate::readiness::require_drained(w, context.phase())?;
    let a = w.strict.as_mut().ok_or("strict session is not open")?;
    a.authenticate(host)?;
    let lease = a.active.as_ref().ok_or("phase has no outstanding lease")?;
    if lease.context != context || !lease.claimed {
        return Err("phase.finish requires the claimed current context".into());
    }
    if a.terminal.is_some() && w.destruction.is_active() {
        crate::destruction::LifetimeRequest::End {}.execute(w)?;
    }
    let a = w.strict.as_mut().unwrap();
    a.stage = context.phase.next(a.remaining);
    a.active = None;
    a.bump()?;
    Ok(status(w))
}

pub(crate) fn cancel(
    w: &mut DisunityWorld,
    host: &Host,
    context: Context,
) -> Result<Value, String> {
    let a = w.strict.as_mut().ok_or("strict session is not open")?;
    a.authenticate(host)?;
    if a.terminal.is_some() {
        return Err("terminal Cleanup cannot be cancelled".into());
    }
    let lease = a.active.as_ref().ok_or("phase has no outstanding lease")?;
    if lease.context != context || lease.claimed {
        return Err("phase.cancel requires the unclaimed current context".into());
    }
    let clock_before = lease.clock_before.clone();
    a.stage = lease.stage_before;
    a.active = None;
    w.clock = clock_before;
    if context.phase == Phase::FixedUpdate {
        let input = a.pending_input.take().ok_or("missing fixed checkpoint")?;
        let clock = a.pending_clock.take().ok_or("missing clock checkpoint")?;
        w.input = input;
        w.clock = clock;
    }
    w.strict.as_mut().unwrap().bump()?;
    Ok(status(w))
}

fn project_clock(w: &mut DisunityWorld, phase: Phase) -> Result<(), String> {
    if phase != Phase::Cleanup {
        w.clock.phase(if phase.uses_fixed_delta() {
            "fixed"
        } else {
            "update"
        })?;
    }
    Ok(())
}
