use super::{Context, Host, Lease, Phase, Stage, status};
use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CleanupReason {
    InfrastructureFailure,
    HostShutdown,
}
#[derive(Clone)]
pub(super) struct Recovery {
    pub claims: u32,
    pub abandoned_tick: Option<u64>,
    pub execution_unknown: u32,
}

pub(crate) fn is_terminal(w: &DisunityWorld) -> bool {
    w.strict.as_ref().is_some_and(|a| a.terminal.is_some())
}

pub(crate) fn begin_cleanup(
    w: &mut DisunityWorld,
    host: &Host,
    _reason: Option<CleanupReason>,
    token: String,
) -> Result<Value, String> {
    let a = w.strict.as_ref().ok_or("strict session is not open")?;
    a.authenticate(host)?;
    if a.terminal.is_some()
        || a.active
            .as_ref()
            .is_some_and(|l| l.claimed && l.context.phase == Phase::Cleanup)
    {
        return Err("Cleanup execution already issued or claimed".into());
    }
    let sequence = a.sequence.checked_add(1).ok_or("phase sequence overflow")?;
    let abandoned_tick = a
        .pending_input
        .as_ref()
        .map(|_| {
            w.clock
                .fixed_frame
                .checked_add(1)
                .ok_or("fixed tick overflow")
        })
        .transpose()?;
    let context = Context {
        world: a.world,
        session: a.session.clone(),
        frame: w.clock.frame,
        fixed_tick: w.clock.fixed_frame,
        phase: Phase::Cleanup,
        sequence,
        token,
    };
    let execution_unknown = crate::readiness::begin_cleanup(w, sequence);
    let a = w.strict.as_mut().unwrap();
    a.bump()?;
    a.active = Some(Lease {
        context: context.clone(),
        claimed: false,
        clock_before: w.clock.clone(),
        stage_before: a.stage,
    });
    a.sequence = sequence;
    a.remaining = 0;
    a.pending_input = None;
    a.pending_clock = None;
    a.stage = Stage::Phase(Phase::Cleanup);
    a.terminal = Some(Recovery {
        claims: 0,
        abandoned_tick,
        execution_unknown,
    });
    Ok(json!({"context":context,"stamp":status(w)}))
}

#[cfg(test)]
mod tests;
