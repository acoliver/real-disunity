//! The scheduler policy of a game's original engine (#147): each setting against the default.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use worldrunner_script::{
    ClockNative, Fraction, GameScripts, NegativeSleep, NewThread, Order, ResumeValue,
    SchedulerNames, SchedulerPolicy, ScriptConfig, WaitKind, ZeroSleep,
};

const GRANTS: [&str; 6] = ["Spawn", "Sleep", "Wakeup", "Kill", "Current", "now"];

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

/// A game with the scheduler under `policy`, granting `extra` natives besides the usual ones.
fn game(policy: &SchedulerPolicy, extra: &[&str]) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts.register_scheduler_with(&names(), policy).unwrap();
    let grants: Vec<&str> = GRANTS.iter().chain(extra).copied().collect();
    scripts.add_source("game", &grants).unwrap();
    scripts
        .run(
            "game",
            "log.lua",
            b"Log = {} function Note(s) Log[#Log + 1] = s .. '@' .. now() end",
        )
        .unwrap();
    scripts
}

fn log(scripts: &mut GameScripts) -> Vec<String> {
    // An empty Lua table has no array form in JSON.
    let value = scripts.eval_json("game", "return Log").unwrap();
    serde_json::from_value(value).unwrap_or_default()
}

fn run(scripts: &mut GameScripts, code: &str) {
    static FILES: AtomicUsize = AtomicUsize::new(0);
    let file = format!("test{}.lua", FILES.fetch_add(1, Ordering::SeqCst));
    scripts.run("game", &file, code.as_bytes()).unwrap();
}

#[test]
fn a_fractional_sleep_rounds_up_by_default_and_truncates_when_asked() {
    for (fraction, expected) in [(Fraction::Ceil, "x@20"), (Fraction::Truncate, "x@19")] {
        let policy = SchedulerPolicy {
            fraction,
            ..SchedulerPolicy::default()
        };
        let mut scripts = game(&policy, &[]);
        run(&mut scripts, "Spawn(function() Sleep(19.53) Note('x') end)");
        scripts.advance(30).unwrap();
        assert_eq!(log(&mut scripts), [expected], "{fraction:?}");
    }
}

#[test]
fn a_negative_sleep_is_an_error_by_default_and_waits_forever_when_asked() {
    let code = "Waiter = Spawn(function() Sleep(-1) Note('woken') end)";
    let mut scripts = game(&SchedulerPolicy::default(), &[]);
    run(&mut scripts, code);
    let failures = scripts.advance(10).unwrap();
    assert!(failures[0].message.contains("non-negative"), "{failures:?}");

    let policy = SchedulerPolicy {
        negative_sleep: NegativeSleep::Forever,
        ..SchedulerPolicy::default()
    };
    let mut scripts = game(&policy, &[]);
    run(&mut scripts, code);
    assert!(scripts.advance(1000).unwrap().is_empty());
    assert!(log(&mut scripts).is_empty());
    assert_eq!(scripts.thread_count(), 1, "still waiting without a timeout");
    run(&mut scripts, "Wakeup(Waiter)");
    scripts.advance(0).unwrap();
    assert_eq!(log(&mut scripts), ["woken@1000"]);
}

#[test]
fn a_zero_sleep_runs_once_per_advance_by_default_and_again_within_it_when_asked() {
    let code = "Spawn(function() for i = 1, 3 do Sleep(0) Note(i) end end)";
    let mut scripts = game(&SchedulerPolicy::default(), &[]);
    run(&mut scripts, code);
    scripts.advance(10).unwrap();
    assert!(log(&mut scripts).is_empty(), "the first run only sleeps");
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["1@10"]);

    let policy = SchedulerPolicy {
        zero_sleep: ZeroSleep::SameAdvance,
        ..SchedulerPolicy::default()
    };
    let mut scripts = game(&policy, &[]);
    run(&mut scripts, code);
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["1@0", "2@0", "3@0"]);
}

#[test]
fn a_loop_of_zero_sleeps_within_an_advance_ends_at_the_limit() {
    let policy = SchedulerPolicy {
        zero_sleep: ZeroSleep::SameAdvance,
        max_zero_sleeps: 50,
        ..SchedulerPolicy::default()
    };
    let mut scripts = game(&policy, &[]);
    run(
        &mut scripts,
        "Spins = 0
         Spawn(function() while true do Spins = Spins + 1 Sleep(0) end end)
         Spawn(function() Sleep(5) Note('other') end)",
    );
    let failures = scripts.advance(10).unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message.contains("zero"), "{failures:?}");
    let spins = scripts.eval_json("game", "return Spins").unwrap();
    assert_eq!(spins, serde_json::json!(51));
    assert_eq!(log(&mut scripts), ["other@5"], "other threads still run");
}

#[test]
fn a_thread_created_in_an_advance_runs_in_it_only_when_asked() {
    let code = "Spawn(function() Sleep(5) Spawn(function() Note('child') end) end)";
    let mut scripts = game(&SchedulerPolicy::default(), &[]);
    run(&mut scripts, code);
    scripts.advance(10).unwrap();
    assert!(log(&mut scripts).is_empty());
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["child@10"]);

    let policy = SchedulerPolicy {
        new_thread: NewThread::SameAdvance,
        ..SchedulerPolicy::default()
    };
    let mut scripts = game(&policy, &[]);
    run(&mut scripts, code);
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["child@5"]);
}

#[test]
fn threads_due_together_run_by_sleep_order_or_by_registration_order() {
    let code = "Spawn(function() Sleep(5) Sleep(5) Note('first') end)
                Spawn(function() Sleep(10) Note('second') end)";
    for (order, expected) in [
        (Order::ByWakeThenReschedule, ["second@10", "first@10"]),
        (
            Order::ByWakeThenClockThenRegistration,
            ["first@10", "second@10"],
        ),
    ] {
        let policy = SchedulerPolicy {
            order,
            ..SchedulerPolicy::default()
        };
        let mut scripts = game(&policy, &[]);
        run(&mut scripts, code);
        scripts.advance(20).unwrap();
        assert_eq!(log(&mut scripts), expected, "{order:?}");
    }
}

fn two_clocks() -> SchedulerPolicy {
    SchedulerPolicy {
        order: Order::ByWakeThenClockThenRegistration,
        clocks: vec!["real".into(), "game".into()],
        spawn_on: vec![ClockNative {
            native: "SpawnGame".into(),
            clock: "game".into(),
        }],
        read_clock: vec![
            ClockNative {
                native: "GameTime".into(),
                clock: "game".into(),
            },
            ClockNative {
                native: "RealTime".into(),
                clock: "real".into(),
            },
        ],
        ..SchedulerPolicy::default()
    }
}

#[test]
fn clocks_keep_their_own_time_and_a_paused_clock_stops() {
    let mut scripts = game(&two_clocks(), &["SpawnGame", "GameTime", "RealTime"]);
    scripts.set_clock("game", 1000).unwrap();
    run(
        &mut scripts,
        "SpawnGame(function() Sleep(10) Note('game') end)
         Spawn(function() Sleep(10) Note('real') end)",
    );
    scripts.advance(10).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["real@10", "game@1010"],
        "real ties first"
    );
    assert_eq!(scripts.clock_now("real"), Some(10));
    assert_eq!(scripts.clock_now("game"), Some(1010));
    assert_eq!(scripts.now(), 10, "the first clock is the default");

    run(
        &mut scripts,
        "SpawnGame(function() Sleep(5) Note('game2') end)
         Spawn(function() Sleep(5) Note('real2') end)",
    );
    scripts.set_clock_paused("game", true).unwrap();
    scripts.advance(20).unwrap();
    assert_eq!(scripts.clock_now("real"), Some(30));
    assert_eq!(scripts.clock_now("game"), Some(1010));
    assert_eq!(log(&mut scripts).last().unwrap(), "real2@15");
    scripts.set_clock_paused("game", false).unwrap();
    scripts.advance(5).unwrap();
    assert_eq!(log(&mut scripts).last().unwrap(), "game2@1015");
    assert!(scripts.set_clock("nonesuch", 1).is_err());
}

#[test]
fn a_native_reads_the_time_of_a_named_clock() {
    let mut scripts = game(&two_clocks(), &["SpawnGame", "GameTime", "RealTime"]);
    scripts.set_clock("game", 500).unwrap();
    run(
        &mut scripts,
        "Spawn(function() Sleep(7) Note(GameTime() .. '/' .. RealTime()) end)",
    );
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["507/7@7"]);
}

fn waits(unlinks: &Arc<AtomicUsize>) -> (GameScripts, SchedulerPolicy) {
    let policy = SchedulerPolicy {
        waits: vec![WaitKind {
            native: "WaitRaw".into(),
            timeout_value: ResumeValue::Boolean(false),
            on_resume: Some("Unlink".into()),
        }],
        wake: Some("WakeRaw".into()),
        ..SchedulerPolicy::default()
    };
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    let count = Arc::clone(unlinks);
    scripts
        .register_native("Unlink", move |_, _thread: mlua::Thread| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    let seen = Arc::clone(unlinks);
    scripts
        .register_native("Unlinks", move |_, ()| Ok(seen.load(Ordering::SeqCst)))
        .unwrap();
    scripts.register_scheduler_with(&names(), &policy).unwrap();
    let grants: Vec<&str> = GRANTS
        .iter()
        .chain(&["WaitRaw", "WakeRaw", "Unlinks"])
        .copied()
        .collect();
    scripts.add_source("game", &grants).unwrap();
    scripts
        .run(
            "game",
            "log.lua",
            b"Log = {} function Note(s) Log[#Log + 1] = s .. '@' .. now() end
              function WaitWakeup(timeout) return WaitRaw(timeout) end
              function Wake(thread, ...) return WakeRaw(thread, true, ...) end",
        )
        .unwrap();
    (scripts, policy)
}

#[test]
fn a_wait_resumes_with_the_wake_values_or_the_timeout_value() {
    let unlinks = Arc::new(AtomicUsize::new(0));
    let (mut scripts, _) = waits(&unlinks);
    run(
        &mut scripts,
        "Waiter = Spawn(function() Note('woken ' .. tostring(WaitWakeup()) .. ' unlinks ' .. Unlinks()) end)
         Spawn(function() Note('timeout ' .. tostring(WaitWakeup(20)) .. ' unlinks ' .. Unlinks()) end)
         Spawn(function() Sleep(5) Wake(Waiter) end)",
    );
    scripts.advance(100).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["woken true unlinks 1@5", "timeout false unlinks 2@20"]
    );
    assert_eq!(unlinks.load(Ordering::SeqCst), 2, "once per resumed wait");
}

#[test]
fn the_host_wakes_a_waiting_thread_with_values() {
    let unlinks = Arc::new(AtomicUsize::new(0));
    let (mut scripts, _) = waits(&unlinks);
    run(
        &mut scripts,
        "Waiter = Spawn(function() local a, b = WaitRaw() Note(a .. b) end)
         Sleeper = Spawn(function() Sleep(1000) end)
         function GetWaiter() return Waiter end
         function GetSleeper() return Sleeper end",
    );
    scripts.advance(1).unwrap();
    let waiter: mlua::Thread = scripts.call("GetWaiter", ()).unwrap();
    assert!(scripts.wake_thread(
        &waiter,
        vec![mlua::Value::Integer(4), mlua::Value::Integer(2)]
    ));
    let sleeper: mlua::Thread = scripts.call("GetSleeper", ()).unwrap();
    assert!(
        !scripts.wake_thread(&sleeper, vec![]),
        "a sleeper is not in a wait"
    );
    scripts.advance(0).unwrap();
    assert_eq!(log(&mut scripts), ["42@1"]);
}

#[test]
fn a_save_refuses_threads_the_snapshot_cannot_describe_yet() {
    let unlinks = Arc::new(AtomicUsize::new(0));
    let (mut scripts, _) = waits(&unlinks);
    run(&mut scripts, "Spawn(function() WaitWakeup(50) end)");
    scripts.advance(1).unwrap();
    let error = scripts.save(&[], &[]).unwrap_err();
    assert!(error.to_string().contains("waits"), "{error}");
}
