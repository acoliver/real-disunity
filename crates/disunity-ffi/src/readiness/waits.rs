use super::{Phase, routines::State};
use crate::{DisunityWorld, ratio::Ratio};
use serde::Deserialize;
use serde_json::Value;
#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Wait {
    Next {
        #[serde(skip)]
        frame: u64,
    },
    Fixed {
        #[serde(skip)]
        tick: u64,
    },
    EndFrame {
        #[serde(skip)]
        frame: u64,
    },
    Scaled {
        seconds: Ratio,
        #[serde(skip)]
        frame: u64,
    },
    Unscaled {
        seconds: Ratio,
        #[serde(skip)]
        frame: u64,
    },
    Join {
        routine: u64,
        #[serde(skip)]
        frame: u64,
    },
}
pub(super) fn suspend(
    w: &mut DisunityWorld,
    id: u64,
    wait: Option<Wait>,
    phase: Phase,
) -> Result<Value, String> {
    let r = w
        .readiness
        .routines
        .get(&id)
        .ok_or("unknown or stale routine")?;
    if !matches!(r.state, State::Suspend) {
        return Err("suspend requires a completed Current claim".into());
    }
    super::nested::suspending(w, id)?;
    let wait = prepare(w, id, wait.unwrap_or(Wait::Next { frame: 0 }), phase)?;
    w.readiness.routines.get_mut(&id).unwrap().state = State::Waiting(wait);
    super::nested::restore(w);
    Ok(super::empty())
}
fn prepare(w: &DisunityWorld, id: u64, wait: Wait, phase: Phase) -> Result<Wait, String> {
    let next_frame = w
        .clock
        .frame
        .checked_add(1)
        .ok_or("readiness frame overflow")?;
    let clock = w
        .clock
        .rational
        .as_ref()
        .ok_or("native rational clock required")?;
    Ok(match wait {
        Wait::Next { .. } => Wait::Next { frame: next_frame },
        Wait::Fixed { .. } => Wait::Fixed {
            tick: w
                .clock
                .fixed_frame
                .checked_add(if phase == Phase::FixedCoroutines {
                    2
                } else {
                    1
                })
                .ok_or("readiness tick overflow")?,
        },
        Wait::EndFrame { .. } => Wait::EndFrame {
            frame: if phase == Phase::EndOfFrame {
                next_frame
            } else {
                w.clock.frame
            },
        },
        Wait::Scaled { seconds, .. } => Wait::Scaled {
            seconds: clock.time.add(seconds.checked()?)?,
            frame: next_frame,
        },
        Wait::Unscaled { seconds, .. } => Wait::Unscaled {
            seconds: clock.unscaled_time.add(seconds.checked()?)?,
            frame: next_frame,
        },
        Wait::Join { routine, .. } => {
            check_join(w, id, routine)?;
            Wait::Join {
                routine,
                frame: next_frame,
            }
        }
    })
}
fn check_join(w: &DisunityWorld, id: u64, mut target: u64) -> Result<(), String> {
    for _ in 0..=super::LIMIT {
        if target == id || super::nested::ancestor(w, target) {
            return Err("routine join cycle".into());
        }
        let r = w
            .readiness
            .routines
            .get(&target)
            .ok_or("unknown or stale join target")?;
        if let State::Waiting(Wait::Join { routine, .. }) = r.state {
            target = routine;
        } else {
            return Ok(());
        }
    }
    Err("routine join budget exceeded".into())
}
pub(super) fn ready(w: &DisunityWorld, wait: &Wait, phase: Phase) -> bool {
    let clock = w.clock.rational.as_ref().unwrap();
    match wait {
        Wait::Fixed { tick } => phase == Phase::FixedCoroutines && w.clock.fixed_frame + 1 >= *tick,
        Wait::EndFrame { frame } => phase == Phase::EndOfFrame && w.clock.frame >= *frame,
        Wait::Next { frame } => phase == Phase::UpdateCoroutines && w.clock.frame >= *frame,
        Wait::Scaled { seconds, frame } => {
            phase == Phase::UpdateCoroutines && w.clock.frame >= *frame && clock.time >= *seconds
        }
        Wait::Unscaled { seconds, frame } => {
            phase == Phase::UpdateCoroutines
                && w.clock.frame >= *frame
                && clock.unscaled_time >= *seconds
        }
        Wait::Join { routine, frame } => {
            phase == Phase::UpdateCoroutines
                && w.clock.frame >= *frame
                && w.readiness
                    .routines
                    .get(routine)
                    .is_some_and(|r| matches!(r.state, State::Terminal))
        }
    }
}
