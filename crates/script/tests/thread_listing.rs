//! The host's listing of scheduler threads, and wakes that check the kind of wait (#147).
//!
//! A game that binds the scheduler as the original thread library does needs its tests, and
//! its tools, to read each thread's clock, flags, source, wait and wake time, and needs
//! `Wakeup` to wake only a `WaitWakeup` thread and the message wake only a `WaitMsg` thread.

use std::sync::atomic::{AtomicUsize, Ordering};

use mlua::Thread;
use worldrunner_script::{
    FlaggedSpawn, GameScripts, ResumeValue, SchedulerNames, SchedulerPolicy, ScriptConfig,
    ThreadInfo, ThreadWait, WaitKind, WakeNative,
};

const GRANTS: [&str; 12] = [
    "Spawn",
    "Sleep",
    "Wakeup",
    "Kill",
    "Current",
    "now",
    "SpawnGame",
    "SetSource",
    "WaitWakeup",
    "WaitMsg",
    "WakeWakeup",
    "WakeMsg",
];

const GAME_FLAGS: i64 = 0x24;

fn names() -> SchedulerNames {
    SchedulerNames {
        spawn: "Spawn".into(),
        sleep: "Sleep".into(),
        wakeup: "Wakeup".into(),
        kill: "Kill".into(),
        current: "Current".into(),
        now: "now".into(),
    }
}

fn policy() -> SchedulerPolicy {
    let wait = |native: &str| WaitKind {
        native: native.into(),
        timeout_value: ResumeValue::Boolean(false),
        on_resume: None,
    };
    let wake = |native: &str, wait: &str| WakeNative {
        native: native.into(),
        wait: wait.into(),
    };
    SchedulerPolicy {
        clocks: vec!["real".into(), "game".into()],
        spawn_flagged: vec![FlaggedSpawn {
            native: "SpawnGame".into(),
            clock: "game".into(),
            flags: GAME_FLAGS,
        }],
        set_source: Some("SetSource".into()),
        waits: vec![wait("WaitWakeup"), wait("WaitMsg")],
        wake_kinds: vec![wake("WakeWakeup", "WaitWakeup"), wake("WakeMsg", "WaitMsg")],
        ..SchedulerPolicy::default()
    }
}

const PRELUDE: &[u8] = br#"
Log = {}
function Note(...)
    local parts = {}
    for i = 1, select('#', ...) do parts[i] = tostring((select(i, ...))) end
    Log[#Log + 1] = table.concat(parts, ' ')
end
"#;

fn game() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_scheduler_with(&names(), &policy())
        .unwrap();
    scripts.add_source("game", &GRANTS).unwrap();
    scripts.run("game", "prelude.lua", PRELUDE).unwrap();
    scripts
}

fn run(scripts: &mut GameScripts, code: &str) {
    static FILES: AtomicUsize = AtomicUsize::new(0);
    let file = format!("case{}.lua", FILES.fetch_add(1, Ordering::SeqCst));
    scripts.run("game", &file, code.as_bytes()).unwrap();
}

fn log(scripts: &mut GameScripts) -> Vec<String> {
    let value = scripts.eval_json("game", "return Log").unwrap();
    serde_json::from_value(value).unwrap_or_default()
}

fn handle(scripts: &mut GameScripts, global: &str) -> Thread {
    run(scripts, &format!("function Handle() return {global} end"));
    scripts
        .call("Handle", ())
        .unwrap_or_else(|_| panic!("{global} is a thread"))
}

fn waits(infos: &[ThreadInfo]) -> Vec<ThreadWait> {
    infos.iter().map(|info| info.wait.clone()).collect()
}

fn kind(native: &str) -> ThreadWait {
    ThreadWait::Wait(native.into())
}

const THREE: &str = "
    Sleeper = Spawn(function() Sleep(50) end)
    Waiter = SpawnGame(function() Note('waiter', WaitWakeup()) end)
    Messenger = SpawnGame(function() Note('messenger', WaitMsg(30)) end)
    SetSource(Messenger, 'mod')
";

#[test]
fn an_empty_scheduler_lists_no_threads() {
    assert!(game().threads().is_empty());
}

#[test]
fn threads_not_yet_run_wait_for_the_next_advance() {
    let mut scripts = game();
    scripts.set_clock("game", 1000).unwrap();
    run(&mut scripts, THREE);
    let infos = scripts.threads();
    assert_eq!(
        waits(&infos),
        [
            ThreadWait::Deferred,
            ThreadWait::Deferred,
            ThreadWait::Deferred
        ]
    );
    assert_eq!(infos.iter().map(|i| i.order).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(infos[0].due, Some(0), "due on the thread's own clock");
    assert_eq!(infos[1].due, Some(1000));
}

#[test]
fn the_listing_reads_each_threads_clock_flags_source_wait_and_wake_time() {
    let mut scripts = game();
    scripts.set_clock("game", 1000).unwrap();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    let infos = scripts.threads();

    let clocks: Vec<&str> = infos.iter().map(|i| i.clock.as_str()).collect();
    assert_eq!(clocks, ["real", "game", "game"]);
    assert_eq!(
        waits(&infos),
        [ThreadWait::Sleep, kind("WaitWakeup"), kind("WaitMsg")]
    );
    let due: Vec<Option<u64>> = infos.iter().map(|i| i.due).collect();
    assert_eq!(
        due,
        [Some(50), None, Some(1030)],
        "None waits without a timeout"
    );
    let flags: Vec<i64> = infos.iter().map(|i| i.flags).collect();
    assert_eq!(flags, [0, GAME_FLAGS, GAME_FLAGS]);
    let sources: Vec<&str> = infos.iter().map(|i| i.source.as_str()).collect();
    assert_eq!(sources, ["", "", "mod"]);
    assert_eq!(infos.iter().map(|i| i.order).collect::<Vec<_>>(), [0, 1, 2]);
    for (info, global) in infos.iter().zip(["Sleeper", "Waiter", "Messenger"]) {
        assert_eq!(info.thread, handle(&mut scripts, global));
    }
}

#[test]
fn the_listing_stays_in_registration_order_when_wake_times_differ() {
    let mut scripts = game();
    run(
        &mut scripts,
        "Late = Spawn(function() Sleep(90) end)
         Early = Spawn(function() Sleep(10) end)",
    );
    scripts.advance(1).unwrap();
    let infos = scripts.threads();
    assert_eq!(infos[0].thread, handle(&mut scripts, "Late"));
    assert_eq!(infos[1].thread, handle(&mut scripts, "Early"));
    assert_eq!(infos[0].due, Some(90));
    assert_eq!(infos[1].due, Some(10));
}

#[test]
fn a_woken_thread_is_ready_until_it_runs_and_a_killed_thread_is_gone() {
    let mut scripts = game();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    run(&mut scripts, "Wakeup(Sleeper) Kill(Messenger)");
    let infos = scripts.threads();
    assert_eq!(waits(&infos), [ThreadWait::Ready, kind("WaitWakeup")]);
    assert_eq!(infos[0].due, Some(1), "due now on its own clock");
    scripts.advance(0).unwrap();
    assert_eq!(scripts.threads().len(), 1, "the sleeper finished");
}

#[test]
fn a_sleeper_without_a_timeout_lists_as_asleep_with_no_wake_time() {
    let mut scripts = game();
    run(&mut scripts, "Spawn(function() Sleep(math.huge) end)");
    scripts.advance(5).unwrap();
    let infos = scripts.threads();
    assert_eq!(waits(&infos), [ThreadWait::Sleep]);
    assert_eq!(infos[0].due, None);
}

#[test]
fn each_wake_native_wakes_only_threads_in_its_kind_of_wait() {
    let mut scripts = game();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    run(
        &mut scripts,
        "Note('msg wakes waiter', WakeMsg(Waiter, true))
         Note('wakeup wakes messenger', WakeWakeup(Messenger, true))
         Note('wakeup wakes sleeper', WakeWakeup(Sleeper, true))
         Note('wakeup wakes a number', WakeWakeup(5, true))
         Note('wakeup wakes nil', WakeWakeup(nil, true))",
    );
    assert_eq!(
        log(&mut scripts),
        [
            "msg wakes waiter false",
            "wakeup wakes messenger false",
            "wakeup wakes sleeper false",
            "wakeup wakes a number false",
            "wakeup wakes nil false",
        ]
    );
    let infos = scripts.threads();
    assert_eq!(
        waits(&infos),
        [ThreadWait::Sleep, kind("WaitWakeup"), kind("WaitMsg")],
        "no refused wake changed a thread"
    );
}

#[test]
fn a_wake_in_the_right_kind_resumes_the_thread_with_the_values() {
    let mut scripts = game();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    run(
        &mut scripts,
        "Note('wakeup', WakeWakeup(Waiter, true, 'a', 'b'))
         Note('msg', WakeMsg(Messenger, true, 'c'))",
    );
    scripts.advance(0).unwrap();
    assert_eq!(
        log(&mut scripts),
        [
            "wakeup true",
            "msg true",
            "waiter true a b",
            "messenger true c"
        ]
    );
}

#[test]
fn a_thread_that_is_already_woken_is_not_woken_again() {
    let mut scripts = game();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    run(
        &mut scripts,
        "Note(WakeWakeup(Waiter, true, 'first'), WakeWakeup(Waiter, true, 'second'))",
    );
    scripts.advance(0).unwrap();
    assert_eq!(log(&mut scripts), ["true false", "waiter true first"]);
}

#[test]
fn a_thread_that_timed_out_and_ended_cannot_be_woken() {
    let mut scripts = game();
    run(&mut scripts, THREE);
    scripts.advance(100).unwrap();
    run(&mut scripts, "Note(WakeMsg(Messenger, true))");
    assert_eq!(log(&mut scripts), ["messenger false", "false"]);
}

#[test]
fn the_generic_wake_still_wakes_any_wait() {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    let policy = SchedulerPolicy {
        wake: Some("WakeRaw".into()),
        ..policy()
    };
    scripts.register_scheduler_with(&names(), &policy).unwrap();
    let grants: Vec<&str> = GRANTS.iter().copied().chain(["WakeRaw"]).collect();
    scripts.add_source("game", &grants).unwrap();
    scripts.run("game", "prelude.lua", PRELUDE).unwrap();
    run(&mut scripts, THREE);
    scripts.advance(1).unwrap();
    run(&mut scripts, "Note(WakeRaw(Messenger, 'any'))");
    assert_eq!(log(&mut scripts).last().unwrap(), "true");
}

#[test]
fn a_wake_native_naming_an_unknown_wait_is_refused() {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    let policy = SchedulerPolicy {
        wake_kinds: vec![WakeNative {
            native: "WakeNothing".into(),
            wait: "NoSuchWait".into(),
        }],
        ..SchedulerPolicy::default()
    };
    let error = scripts
        .register_scheduler_with(&names(), &policy)
        .unwrap_err();
    assert!(error.to_string().contains("NoSuchWait"), "{error}");
}
