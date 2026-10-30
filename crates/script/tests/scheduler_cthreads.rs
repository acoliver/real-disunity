//! A game that binds the scheduler as the original engine's thread library does (#147).
//!
//! The scenarios and expected traces come from the cthreads tests of rd-stranded
//! (`tests/natives.rs`), which record what the original engine does: a real and a game clock
//! with a pause, `WaitWakeup` and `WaitMsg` with timeouts that resume with `false`, wakes that
//! resume with `true` and their values, and threads due together run real before game and then
//! in registration order. The message tables live in the game's own Lua, as they do there.

use mlua::{Function, Thread};
use worldrunner_script::{
    ClockNative, Fraction, GameScripts, NegativeSleep, NewThread, Order, ResumeValue,
    SchedulerNames, SchedulerPolicy, ScriptConfig, WaitKind, ZeroSleep,
};

const GRANTS: [&str; 11] = [
    "CreateRealTimeThread",
    "CreateGameTimeThread",
    "Sleep",
    "WakeSleeper",
    "DeleteThread",
    "CurrentThread",
    "now",
    "GameTime",
    "RealTime",
    "WaitWakeup",
    "WaitMsgRaw",
];

const PRELUDE: &[u8] = br#"
Log = {}
ByMessage, ByThread = {}, {}
function note(...)
    local parts = {}
    for i = 1, select('#', ...) do parts[i] = tostring((select(i, ...))) end
    Log[#Log + 1] = table.concat(parts, ' ')
end
function Wakeup(thread, ...) return WakeRaw(thread, true, ...) end
function WaitMsg(message, timeout)
    local thread = CurrentThread()
    local list = ByMessage[message] or {}
    ByMessage[message] = list
    list[#list + 1] = thread
    ByThread[thread] = message
    return WaitMsgRaw(timeout)
end
function UnlinkThread(thread)
    local message = ByThread[thread]
    if message == nil then return end
    ByThread[thread] = nil
    local list = ByMessage[message]
    for i = #list, 1, -1 do
        if list[i] == thread then table.remove(list, i) end
    end
    if #list == 0 then ByMessage[message] = nil end
end
function MsgThreads(message, ...)
    local list = ByMessage[message]
    if not list then return end
    ByMessage[message] = nil
    for _, thread in ipairs(list) do
        ByThread[thread] = nil
        WakeRaw(thread, true, ...)
    end
end
"#;

fn original_policy() -> SchedulerPolicy {
    let on = |native: &str, clock: &str| ClockNative {
        native: native.into(),
        clock: clock.into(),
    };
    let wait = |native: &str| WaitKind {
        native: native.into(),
        timeout_value: ResumeValue::Boolean(false),
        on_resume: Some("UnlinkOnResume".into()),
    };
    SchedulerPolicy {
        fraction: Fraction::Truncate,
        negative_sleep: NegativeSleep::Forever,
        zero_sleep: ZeroSleep::SameAdvance,
        new_thread: NewThread::SameAdvance,
        order: Order::ByWakeThenClockThenRegistration,
        max_zero_sleeps: 100,
        clocks: vec!["real".into(), "game".into()],
        spawn_on: vec![on("CreateGameTimeThread", "game")],
        read_clock: vec![on("GameTime", "game"), on("RealTime", "real")],
        waits: vec![wait("WaitWakeup"), wait("WaitMsgRaw")],
        wake: Some("WakeRaw".into()),
        ..SchedulerPolicy::default()
    }
}

fn game() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_native("UnlinkOnResume", |lua, thread: Thread| {
            let unlink: Function = lua.globals().get("UnlinkThread")?;
            unlink.call::<()>(thread)
        })
        .unwrap();
    let names = SchedulerNames {
        spawn: "CreateRealTimeThread".into(),
        sleep: "Sleep".into(),
        wakeup: "WakeSleeper".into(),
        kill: "DeleteThread".into(),
        current: "CurrentThread".into(),
        now: "now".into(),
    };
    scripts
        .register_scheduler_with(&names, &original_policy())
        .unwrap();
    let grants: Vec<&str> = GRANTS.iter().copied().chain(["WakeRaw"]).collect();
    scripts.add_source("game", &grants).unwrap();
    scripts.run("game", "prelude.lua", PRELUDE).unwrap();
    scripts
}

fn run(scripts: &mut GameScripts, code: &str) {
    scripts.run("game", "case.lua", code.as_bytes()).unwrap();
}

fn log(scripts: &mut GameScripts) -> Vec<String> {
    let value = scripts.eval_json("game", "return Log").unwrap();
    serde_json::from_value(value).unwrap_or_default()
}

#[test]
fn threads_wake_in_time_order_then_registration_order() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateGameTimeThread(function() Sleep(30) note('A', GameTime()) end)
         CreateGameTimeThread(function() Sleep(10) note('B', GameTime()) end)
         CreateGameTimeThread(function() Sleep(10) note('C', GameTime()) end)",
    );
    assert!(scripts.advance(50).unwrap().is_empty());
    assert_eq!(log(&mut scripts), ["B 10", "C 10", "A 30"]);
    assert_eq!(scripts.clock_now("game"), Some(50));
}

#[test]
fn the_two_clocks_tie_real_first_and_a_paused_game_clock_stops() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateGameTimeThread(function() Sleep(10) note('game', now()) end)
         CreateRealTimeThread(function() Sleep(10) note('real', now()) end)",
    );
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["real 10", "game 10"]);
    // A second text needs its own chunk name.
    scripts
        .run(
            "game",
            "second.lua",
            b"CreateGameTimeThread(function() Sleep(5) note('game2') end)
              CreateRealTimeThread(function() Sleep(5) note('real2') end)",
        )
        .unwrap();
    scripts.set_clock_paused("game", true).unwrap();
    scripts.advance(20).unwrap();
    assert_eq!(
        (scripts.clock_now("real"), scripts.clock_now("game")),
        (Some(30), Some(10))
    );
    assert_eq!(log(&mut scripts).last().unwrap(), "real2");
    scripts.set_clock_paused("game", false).unwrap();
    scripts.advance(5).unwrap();
    assert_eq!(log(&mut scripts).last().unwrap(), "game2");
}

#[test]
fn wakeup_delivers_true_and_its_values_and_a_timeout_delivers_false() {
    let mut scripts = game();
    run(
        &mut scripts,
        "waiter = CreateRealTimeThread(function() note('woken', tostring(WaitWakeup()), 'at', now()) end)
         CreateRealTimeThread(function() note('timeout', tostring(WaitWakeup(20)), 'at', now()) end)
         CreateRealTimeThread(function() Sleep(5) Wakeup(waiter) end)",
    );
    scripts.advance(100).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["woken true at 5", "timeout false at 20"]
    );
}

#[test]
fn wait_msg_returns_the_message_values_and_times_out_with_false() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateRealTimeThread(function() note('got', tostring(select(1, WaitMsg('m'))), select(2, WaitMsg('m'))) end)
         CreateRealTimeThread(function() note('late', tostring(WaitMsg('never', 15)), now()) end)
         CreateRealTimeThread(function() Sleep(4) MsgThreads('m', 'a', 'b') end)
         CreateRealTimeThread(function() Sleep(8) MsgThreads('m', 'c', 'd') end)",
    );
    scripts.advance(30).unwrap();
    assert_eq!(log(&mut scripts), ["got true c d", "late false 15"]);
    let empty = scripts
        .eval_json(
            "game",
            "return next(ByMessage) == nil and next(ByThread) == nil",
        )
        .unwrap();
    assert_eq!(
        empty,
        serde_json::json!(true),
        "the game's tables are unlinked after the wake and after the timeout"
    );
}

#[test]
fn waiters_of_one_message_wake_in_the_order_they_waited() {
    let mut scripts = game();
    run(
        &mut scripts,
        "for _, name in ipairs{'x', 'y', 'z'} do
           CreateRealTimeThread(function() WaitMsg('go') note(name) end)
         end
         CreateRealTimeThread(function() Sleep(1) MsgThreads('go') end)",
    );
    scripts.advance(5).unwrap();
    assert_eq!(log(&mut scripts), ["x", "y", "z"]);
}

#[test]
fn fractions_truncate_negative_sleeps_wait_and_a_zero_sleep_loop_is_limited() {
    let mut scripts = game();
    run(
        &mut scripts,
        "Spins = 0
         CreateRealTimeThread(function() Sleep(19.53) note('fraction', now()) end)
         Forever = CreateRealTimeThread(function() Sleep(-5) note('forever woken') end)
         CreateRealTimeThread(function() while true do Spins = Spins + 1 Sleep(0) end end)",
    );
    let failures = scripts.advance(30).unwrap();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(
        scripts.eval_json("game", "return Spins").unwrap(),
        serde_json::json!(101),
        "the loop stops at the limit within one advance"
    );
    assert_eq!(log(&mut scripts), ["fraction 19"]);
    assert_eq!(
        scripts.thread_count(),
        1,
        "the negative sleeper still waits"
    );
}
