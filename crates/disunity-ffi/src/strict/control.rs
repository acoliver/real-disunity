use super::{Authority, Host, PROFILE, Phase, Stage, status};
use crate::{DisunityWorld, destruction::LifetimeRequest, input::projection, ratio::Ratio};
use serde_json::{Value, json};

fn auth<'a>(w: &'a mut DisunityWorld, host: &Host) -> Result<&'a mut Authority, String> {
    let a = w.strict.as_mut().ok_or("strict session is not open")?;
    a.authenticate(host)?;
    Ok(a)
}
pub(crate) fn open(
    w: &mut DisunityWorld,
    world: u64,
    profile: &str,
    seed: String,
) -> Result<Value, String> {
    if w.destruction.is_active()
        || !matches!(
            profile,
            PROFILE | crate::readiness::NESTED_PROFILE | super::IDLE_PROFILE
        )
        || (profile != PROFILE && w.content_revision != 1)
        || w.strict.is_some()
        || w.clock.frame != 0
        || w.clock.fixed_frame != 0
    {
        return Err("strict profile requires a fresh unopened world".into());
    }
    w.clock.set_rational(None, None)?;
    // Session identity is public; the independent remaining entropy is private.
    let (session, key) = seed.split_at(64);
    let a = Authority {
        world,
        profile: if profile == super::IDLE_PROFILE {
            super::IDLE_PROFILE
        } else if profile == crate::readiness::NESTED_PROFILE {
            crate::readiness::NESTED_PROFILE
        } else {
            PROFILE
        },
        session: session.into(),
        host_key: key.into(),
        stage: Stage::Phase(Phase::Initialize),
        active: None,
        sequence: 0,
        revision: 0,
        claims: 0,
        remaining: 0,
        pending_input: None,
        pending_clock: None,
        terminal: None,
    };
    let result = json!({"session":a.session,"host_key":a.host_key,"profile":a.profile,
        "supported_phases":super::schedule::phases()});
    w.strict = Some(a);
    Ok(result)
}
pub(crate) fn begin(w: &mut DisunityWorld, host: &Host, dt: Ratio) -> Result<Value, String> {
    let a = auth(w, host)?;
    a.idle_lease()?;
    if a.stage != Stage::Idle {
        return Err("frame.open requires initialized idle session".into());
    }
    w.clock.begin_rational(dt)?;
    w.input.advance(
        projection::Phase::Dynamic,
        w.clock.endpoint()?,
        w.clock.frame,
    )?;
    LifetimeRequest::Begin {}.execute(w)?;
    let a = w.strict.as_mut().unwrap();
    a.remaining = w.clock.fixed_steps;
    a.claims = 0;
    a.stage = Stage::Phase(if a.remaining > 0 {
        Phase::FixedUpdate
    } else {
        Phase::Update
    });
    a.bump()?;
    Ok(json!({"time":w.clock,"stamp":status(w)}))
}
pub(crate) fn configure(
    w: &mut DisunityWorld,
    host: &Host,
    scale: Option<Ratio>,
    fixed: Option<Ratio>,
) -> Result<Value, String> {
    let a = auth(w, host)?;
    a.idle_lease()?;
    if !matches!(a.stage, Stage::Idle | Stage::Phase(Phase::Initialize)) {
        return Err("session.configure requires idle session".into());
    }
    let result = w.clock.set_rational(scale, fixed)?;
    w.strict.as_mut().unwrap().bump()?;
    Ok(result)
}
pub(crate) fn commit(
    w: &mut DisunityWorld,
    host: &Host,
    frame: u64,
    tick: u64,
) -> Result<Value, String> {
    let a = auth(w, host)?;
    a.idle_lease()?;
    if a.stage != Stage::Commit
        || a.remaining == 0
        || frame != w.clock.frame
        || Some(tick) != w.clock.fixed_frame.checked_add(1)
    {
        return Err("fixed.commit identity or stage mismatch".into());
    }
    for command in std::mem::take(&mut w.pending_commands) {
        w.apply_command(command)?;
        w.applied_commands.push(command);
    }
    w.physics_step(w.clock.fixed_delta_time)?;
    let a = w.strict.as_mut().unwrap();
    a.pending_input = None;
    a.pending_clock = None;
    a.remaining -= 1;
    a.stage = Stage::Phase(Phase::Contacts);
    a.bump()?;
    Ok(json!({"time":w.clock,"stamp":status(w)}))
}
pub(crate) fn end(w: &mut DisunityWorld, host: &Host, frame: u64) -> Result<Value, String> {
    let a = auth(w, host)?;
    a.idle_lease()?;
    if a.stage != Stage::End || frame != w.clock.frame {
        return Err("frame.end identity or stage mismatch".into());
    }
    LifetimeRequest::End {}.execute(w)?;
    let a = w.strict.as_mut().unwrap();
    a.stage = Stage::Idle;
    a.bump()?;
    Ok(status(w))
}
pub(crate) fn abort(w: &mut DisunityWorld, host: &Host, frame: u64) -> Result<Value, String> {
    auth(w, host)?;
    if frame != w.clock.frame {
        return Err("frame.abort identity mismatch".into());
    }
    close(w, host)
}
pub(crate) fn close(w: &mut DisunityWorld, host: &Host) -> Result<Value, String> {
    let a = auth(w, host)?;
    let input = a.pending_input.take();
    let clock = a.pending_clock.take();
    if let Some(input) = input {
        w.input = input;
    }
    if let Some(clock) = clock {
        w.clock = clock;
    }
    let a = w.strict.as_mut().unwrap();
    a.active = None;
    a.stage = Stage::Closed;
    a.bump()?;
    Ok(status(w))
}
