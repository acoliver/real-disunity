//! Callback payloads and native-owned coroutine waits.
use crate::{DisunityResult, abi, store::Session};
use worldrunner_world_simulation::{CoroutineId, ScriptMessage, TimeSpan, WaitCondition};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCallback {
    pub ticket: u64,
    pub script: u64,
    pub routine: u64,
    pub collider: u64,
    pub other: u64,
    pub phase: u32,
    pub trigger: u32,
    pub input_phase: u32,
    pub point_x: f32,
    pub point_y: f32,
    pub normal_x: f32,
    pub normal_y: f32,
}

pub(crate) fn mutate(
    world: u64,
    f: impl FnOnce(&mut Session) -> Result<(), crate::errors::AbiError>,
) -> DisunityResult {
    let result = {
        let mut store = crate::lock_store();
        store.session_mut(world).and_then(f)
    };
    result.map_or_else(
        |e| abi::translate_error(Some(world), &e),
        |()| DisunityResult::ok(),
    )
}

pub(crate) fn wait(
    kind: u32,
    num: u64,
    den: u64,
) -> Result<WaitCondition, crate::errors::AbiError> {
    match kind {
        0 => Ok(WaitCondition::NextFrame),
        1 => Ok(WaitCondition::FixedUpdate),
        2 | 3 => {
            let span = TimeSpan::new(num, den).map_err(crate::errors::AbiError::from)?;
            Ok(if kind == 2 {
                WaitCondition::Scaled(span)
            } else {
                WaitCondition::Unscaled(span)
            })
        }
        _ => Err("unsupported coroutine wait".into()),
    }
}

pub(crate) fn payload(
    session: &Session,
    script: worldrunner_world_simulation::ComponentHandle,
    message: ScriptMessage,
) -> Result<DisunityCallback, crate::errors::AbiError> {
    let mut value = DisunityCallback::default();
    match message {
        ScriptMessage::Coroutine(id) => {
            value.routine = session
                .dispatch
                .routines
                .iter()
                .find_map(|(&handle, &(native, _))| (native == id).then_some(handle))
                .ok_or("coroutine has no managed binding")?;
            value.input_phase = u32::from(session.dispatch.routines[&value.routine].1);
        }
        ScriptMessage::Contact2d(id) => contact_payload(session, script, id, &mut value)?,
        ScriptMessage::FixedUpdate => value.input_phase = 1,
        ScriptMessage::Awake
        | ScriptMessage::OnEnable
        | ScriptMessage::OnDisable
        | ScriptMessage::OnDestroy => {
            value.input_phase = session
                .dispatch
                .payloads
                .values()
                .next_back()
                .map_or(0, |parent| parent.input_phase);
        }
        _ => {}
    }
    Ok(value)
}

fn contact_payload(
    session: &Session,
    script: worldrunner_world_simulation::ComponentHandle,
    id: worldrunner_world_simulation::ContactId,
    value: &mut DisunityCallback,
) -> Result<(), crate::errors::AbiError> {
    use worldrunner_world_simulation::physics2d::ContactPhase;
    let contact = session
        .native
        .contact_event(id)
        .map_err(crate::errors::AbiError::from)?;
    let owner = session
        .native
        .component(script)
        .map_err(crate::errors::AbiError::from)?
        .object;
    let left = owner == contact.object_a
        || contact.body_a.is_some_and(|body| {
            session
                .native
                .component(body)
                .is_ok_and(|c| c.object == owner)
        });
    let (this, other, normal) = if left {
        (contact.collider_a, contact.collider_b, contact.normal)
    } else {
        (contact.collider_b, contact.collider_a, -contact.normal)
    };
    let handle = |native| {
        session
            .dispatch
            .components
            .iter()
            .find_map(|(&handle, &component)| (component == native).then_some(handle))
            .ok_or("contact component has no managed binding")
    };
    value.collider = handle(this)?;
    value.other = handle(other)?;
    value.phase = match contact.phase {
        ContactPhase::Enter => 0,
        ContactPhase::Stay => 1,
        ContactPhase::Exit => 2,
    };
    value.trigger = u32::from(contact.trigger);
    value.input_phase = 1;
    value.point_x = contact.point.x;
    value.point_y = contact.point.y;
    value.normal_x = normal.x;
    value.normal_y = normal.y;
    Ok(())
}

/// # Safety
/// `output` is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_callback_read(
    world: u64,
    script: u64,
    output: *mut DisunityCallback,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let value = session
            .dispatch
            .payloads
            .values()
            .rev()
            .find(|p| p.script == script)
            .ok_or("script has no suspended callback")?;
        unsafe { *output = *value };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_callback_wait(
    world: u64,
    ticket: u64,
    kind: u32,
    num: u64,
    den: u64,
) -> DisunityResult {
    mutate(world, |session| {
        let wait = wait(kind, num, den)?;
        let payload = session
            .dispatch
            .payloads
            .get(&ticket)
            .ok_or(worldrunner_world_simulation::WorldError::WrongCallback)?;
        if payload.routine == 0 {
            return Err("only coroutines may yield".into());
        }
        if let Some(entry) = session.dispatch.routines.get_mut(&payload.routine) {
            entry.1 = kind == 1;
        }
        session.dispatch.waits.insert(ticket, wait);
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one coroutine identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_coroutine_start(
    world: u64,
    script: u64,
    kind: u32,
    num: u64,
    den: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let wait = wait(kind, num, den)?;
        let script = session
            .dispatch
            .scripts
            .iter()
            .find_map(|(&native, &id)| (id == script).then_some(native))
            .ok_or("script binding missing")?;
        let identity = session
            .dispatch
            .next_routine
            .checked_add(1)
            .ok_or("coroutine identity exhausted")?;
        let id = session
            .native
            .start_coroutine(script, wait)
            .map_err(crate::errors::AbiError::from)?;
        session.dispatch.next_routine = identity;
        session.dispatch.routines.insert(identity, (id, kind == 1));
        unsafe { *output = identity };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_coroutine_stop(world: u64, routine: u64) -> DisunityResult {
    mutate(world, |session| {
        let &(id, _) = session
            .dispatch
            .routines
            .get(&routine)
            .ok_or("coroutine binding missing")?;
        session
            .native
            .stop_coroutine(id)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `output` is writable for one identity, or zero when the queue is empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_coroutine_cancelled(
    world: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |session| {
        let cancelled: Vec<CoroutineId> = session.native.take_cancelled_coroutines();
        for native in cancelled {
            if let Some(id) = session
                .dispatch
                .routines
                .iter()
                .find_map(|(&id, &(n, _))| (n == native).then_some(id))
            {
                session.dispatch.routines.remove(&id);
                session.dispatch.cancelled.push_back(id);
            }
        }
        unsafe { *output = session.dispatch.cancelled.pop_front().unwrap_or(0) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_coroutine_ready(world: u64, identity: u64) -> DisunityResult {
    mutate(world, |s| {
        let script = s
            .dispatch
            .scripts
            .iter()
            .find_map(|(&handle, &id)| (id == identity).then_some(handle))
            .ok_or("script binding missing")?;
        if !s.native.coroutine_eligible(script) {
            return Err("script is not ready for coroutine execution".into());
        }
        Ok(())
    })
}
