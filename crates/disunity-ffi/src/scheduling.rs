//! Owned scheduler work over the shared native world. No foreign callback runs here.
use worldrunner_world_simulation::{
    Advance, CallbackOutcome, CallbackTicket, ComponentDesc, ComponentHandle, ScriptFailure,
    ScriptMessage, TimeSpan,
};

use crate::abi::DisunityResult;
use crate::store::{Session, lock_store};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityAdvance {
    /// 0: frame complete, 1: callback, 2: lifecycle complete.
    pub kind: u32,
    pub message: u32,
    pub ticket: u64,
    pub script: u64,
    pub frame: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityClock {
    pub delta: f64,
    pub time: f64,
    pub frame: u64,
}

/// The clock values `DisunityClock` leaves out, all in seconds.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityTime {
    pub unscaled_delta: f64,
    pub unscaled_time: f64,
    pub smooth_delta: f64,
    pub time_scale: f64,
    pub maximum_delta: f64,
}

/// Which clock setting `disunity_world_time_write` changes.
pub mod time_setting {
    pub const TIME_SCALE: u32 = 0;
    pub const MAXIMUM_DELTA: u32 = 1;
}

/// The largest denominator `disunity_world_time_write` accepts for a time scale.
pub const MAX_SCALE_DENOMINATOR: u64 = 100;

/// Maximum owned UTF-8 callback diagnostic; input is clipped before allocation.
pub const MAX_DIAGNOSTIC_BYTES: usize = 65536;

static NEXT_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct Dispatch {
    pub scripts: std::collections::BTreeMap<ComponentHandle, u64>,
    pub tickets: std::collections::BTreeMap<u64, CallbackTicket>,
    pub payloads: std::collections::BTreeMap<u64, crate::callbacks::DisunityCallback>,
    pub waits: std::collections::BTreeMap<u64, worldrunner_world_simulation::WaitCondition>,
    pub routines:
        std::collections::BTreeMap<u64, (worldrunner_world_simulation::CoroutineId, bool)>,
    pub next_routine: u64,
    pub cancelled: std::collections::VecDeque<u64>,
    pub components: std::collections::BTreeMap<u64, ComponentHandle>,
}

fn encode(
    session: &mut Session,
    advance: Advance,
) -> Result<DisunityAdvance, crate::errors::AbiError> {
    let mut result = DisunityAdvance {
        frame: session.native.clock().frame,
        ..Default::default()
    };
    match advance {
        Advance::Complete(_) => {}
        Advance::LifecycleComplete => result.kind = 2,
        Advance::Callback(work) => {
            result.kind = 1;
            result.script = *session
                .dispatch
                .scripts
                .get(&work.script)
                .ok_or("script has no managed binding")?;
            result.message = match work.message {
                ScriptMessage::Awake => 0,
                ScriptMessage::OnEnable => 1,
                ScriptMessage::Start => 2,
                ScriptMessage::FixedUpdate => 3,
                ScriptMessage::Update => 4,
                ScriptMessage::LateUpdate => 5,
                ScriptMessage::OnDisable => 6,
                ScriptMessage::OnDestroy => 7,
                ScriptMessage::Coroutine(_) => 8,
                ScriptMessage::Contact2d(_) => 9,
            };
            record_callback(
                session,
                &mut result,
                work.ticket,
                crate::callbacks::payload(session, work.script, work.message)?,
            )?;
        }
        Advance::AssetCallback(work) => {
            result.kind = 1;
            result.script = work.identity;
            result.message = match work.message {
                ScriptMessage::Awake => 10,
                ScriptMessage::OnEnable => 11,
                // Only the delayed-tasks identity receives Update.
                ScriptMessage::Update => 14,
                // Only the delayed-tasks identity receives LateUpdate: the canvas events.
                ScriptMessage::LateUpdate => 15,
                ScriptMessage::OnDisable => 16,
                ScriptMessage::OnDestroy => 17,
                _ => return Err("invalid asset callback".into()),
            };
            let payload = crate::callbacks::DisunityCallback {
                input_phase: session
                    .dispatch
                    .payloads
                    .values()
                    .next_back()
                    .map_or(0, |p| p.input_phase),
                ..Default::default()
            };
            record_callback(session, &mut result, work.ticket, payload)?;
        }
    }
    Ok(result)
}

fn record_callback(
    session: &mut Session,
    result: &mut DisunityAdvance,
    ticket: CallbackTicket,
    mut payload: crate::callbacks::DisunityCallback,
) -> Result<(), crate::errors::AbiError> {
    result.ticket = NEXT_TICKET
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |next| next.checked_add(1),
        )
        .map_err(|_| "callback identity exhausted")?;
    payload.ticket = result.ticket;
    payload.script = result.script;
    session.dispatch.payloads.insert(result.ticket, payload);
    session.dispatch.tickets.insert(result.ticket, ticket);
    Ok(())
}

pub(crate) fn call(
    world: u64,
    output: *mut DisunityAdvance,
    f: impl FnOnce(&mut Session) -> Result<Advance, crate::errors::AbiError>,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let result = {
        let mut store = lock_store();
        store.session_mut(world).and_then(|session| {
            let result = f(session);
            session
                .objects
                .retain(|_, handle| session.native.object_alive(*handle));
            let retired: Vec<_> = session
                .dispatch
                .tickets
                .iter()
                .filter_map(|(&id, &ticket)| {
                    (!session.native.callback_pending(ticket)).then_some(id)
                })
                .collect();
            for id in retired {
                session.dispatch.tickets.remove(&id);
                session.dispatch.payloads.remove(&id);
                session.dispatch.waits.remove(&id);
            }
            let advance = result?;
            if matches!(advance, Advance::Complete(_)) {
                session
                    .dispatch
                    .scripts
                    .retain(|handle, _| session.native.component(*handle).is_ok());
                session
                    .dispatch
                    .components
                    .retain(|_, handle| session.native.component(*handle).is_ok());
            }
            encode(session, advance)
        })
    };
    match result {
        Ok(value) => {
            unsafe { *output = value };
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_abi_check(revision: u32) -> DisunityResult {
    if revision == crate::DISUNITY_ABI_VERSION {
        DisunityResult::ok()
    } else {
        DisunityResult::invalid_argument()
    }
}

/// # Safety
/// `output` is writable for one u64.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_script_register(
    world: u64,
    object: u64,
    identity: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() || identity == 0 {
        return DisunityResult::invalid_argument();
    }
    let result = {
        let mut store = lock_store();
        store.session_mut(world).and_then(|session| {
            if session.dispatch.scripts.values().any(|&id| id == identity) {
                return Err("duplicate script identity".into());
            }
            let object = session.object(object)?;
            let script = session
                .native
                .add_component(
                    object,
                    ComponentDesc::Script {
                        assembly: "Managed".into(),
                        type_name: identity.to_string(),
                    },
                )
                .map_err(crate::errors::AbiError::from)?;
            session.dispatch.scripts.insert(script, identity);
            Ok(identity)
        })
    };
    match result {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(Some(world), &error),
    }
}

/// Queues `Awake` and `OnEnable` for scripts on `object` and its children that are eligible,
/// then runs them. Whether the object is active is unchanged, so scripts added to an inactive
/// object wait until it is activated, and constructing objects wait for their construction.
/// # Safety
/// `output` is writable for one callback record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_object_wake(
    world: u64,
    object: u64,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    call(world, output, |session| {
        let object = session.object(object)?;
        let mutation = session
            .native
            .wake(object)
            .map_err(crate::errors::AbiError::from)?;
        session
            .native
            .next_lifecycle(mutation)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `output` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_advance(
    world: u64,
    numerator: u64,
    denominator: u64,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    call(world, output, |session| {
        let elapsed =
            TimeSpan::new(numerator, denominator).map_err(crate::errors::AbiError::from)?;
        crate::commands::flush(session)?;
        session
            .native
            .advance(elapsed)
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Input points to `length` bytes; output is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_resume(
    world: u64,
    ticket: u64,
    outcome: u32,
    message: *const u8,
    length: u32,
    output: *mut DisunityAdvance,
) -> DisunityResult {
    if outcome > 1 || (message.is_null() && length != 0) {
        return DisunityResult::invalid_argument();
    }
    let bytes = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(message, length as usize) }
    };
    let Ok(message) = std::str::from_utf8(bytes) else {
        return DisunityResult::invalid_argument();
    };
    let outcome = if outcome == 0 {
        CallbackOutcome::Returned
    } else {
        let mut end = message.len().min(MAX_DIAGNOSTIC_BYTES);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        CallbackOutcome::Failed(ScriptFailure {
            message: message[..end].to_owned(),
        })
    };
    call(world, output, |session| {
        let native_ticket = *session
            .dispatch
            .tickets
            .get(&ticket)
            .ok_or(worldrunner_world_simulation::WorldError::WrongCallback)?;
        crate::commands::flush(session)?;
        let outcome = match (outcome, session.dispatch.waits.get(&ticket)) {
            (CallbackOutcome::Returned, Some(&wait)) => CallbackOutcome::Yielded(wait),
            (outcome, _) => outcome,
        };
        let completed = matches!(outcome, CallbackOutcome::Returned)
            .then(|| session.dispatch.payloads[&ticket].routine)
            .filter(|&id| id != 0);
        let result = session.native.resume(native_ticket, outcome);
        // An accepted failure may end advancement with an error rather than work.
        if !matches!(
            &result,
            Err(worldrunner_world_simulation::WorldError::WrongCallback)
        ) {
            if let Some(id) = completed {
                session.dispatch.routines.remove(&id);
            }
            session.dispatch.tickets.remove(&ticket);
            session.dispatch.payloads.remove(&ticket);
            session.dispatch.waits.remove(&ticket);
        }
        result.map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// `output` is writable for one clock record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_clock(
    world: u64,
    output: *mut DisunityClock,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let store = lock_store();
    let Ok(session) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    let clock = session.native.clock();
    unsafe {
        *output = DisunityClock {
            delta: clock.delta.seconds(),
            time: clock.time.seconds(),
            frame: clock.frame,
        }
    };
    DisunityResult::ok()
}

/// # Safety
/// `output` is writable for one time record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_time_read(
    world: u64,
    output: *mut DisunityTime,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let store = lock_store();
    let Ok(session) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    let clock = session.native.clock();
    let smooth_delta =
        TimeSpan::from_nanos(clock.smooth_delta_nanos).map_or(0.0, TimeSpan::seconds);
    unsafe {
        *output = DisunityTime {
            unscaled_delta: clock.unscaled_delta.seconds(),
            unscaled_time: clock.unscaled_time.seconds(),
            smooth_delta,
            time_scale: clock.time_scale.seconds(),
            maximum_delta: clock.max_delta.seconds(),
        }
    };
    DisunityResult::ok()
}

/// Sets `Time.timeScale` (`setting` 0; a ratio of at most 100 whose denominator is at most
/// 100, because a scale with more digits would multiply the denominators of the clock's exact
/// sums past their bounds) or `Time.maximumDeltaTime` (`setting` 1; above zero) to
/// `numerator / denominator` seconds.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_world_time_write(
    world: u64,
    setting: u32,
    numerator: u64,
    denominator: u64,
) -> DisunityResult {
    crate::callbacks::mutate(world, |session| {
        let span = TimeSpan::new(numerator, denominator).map_err(crate::errors::AbiError::from)?;
        match setting {
            time_setting::TIME_SCALE if denominator <= MAX_SCALE_DENOMINATOR => session
                .native
                .set_time_scale(span)
                .map_err(crate::errors::AbiError::from),
            time_setting::MAXIMUM_DELTA => session
                .native
                .set_max_delta(span)
                .map_err(crate::errors::AbiError::from),
            _ => Err("unknown time setting or a time scale with too many digits".into()),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_callback_check(world: u64, ticket: u64) -> DisunityResult {
    crate::callbacks::mutate(world, |s| {
        if s.dispatch.tickets.contains_key(&ticket) {
            return Ok(());
        }
        if let Some(failure) = s.native.terminal_failure() {
            return Err(crate::errors::AbiError::Aborted(failure.clone()));
        }
        Err(worldrunner_world_simulation::WorldError::WrongCallback.into())
    })
}
