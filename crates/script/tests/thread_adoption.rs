//! Threads restored from an imported original save join the scheduler (#147).
//!
//! A thread restored from an original engine's save was suspended inside `Sleep`,
//! `WaitWakeup` or `WaitMsg`; the restore continues the frames above that native without
//! calling it. The scenarios and expected traces follow the cthreads tests of rd-stranded
//! (`tests/natives.rs`, `lua-heap/tests/cthreads.rs`): a wait that times out resumes with
//! `false` at its deadline on its own clock, a wake resumes with `true` and the values, and
//! threads due together run in the order they were adopted.
//!
//! The imported form is made from an engine save by replacing the frame of the engine's wait
//! function with a pending call to the native, which is how an import describes a thread
//! parked in a native.

use std::sync::{Arc, Mutex};

use lua_resume::capture::{Callee, Snapshot, ThreadData};
use mlua::{Thread, Value};
use worldrunner_script::{
    AdoptWait, Adoption, ClockNative, Fraction, GameScripts, LuaSnapshot, NegativeSleep, NewThread,
    Order, PermanentSpec, ResumeValue, SaveOptions, SchedulerNames, SchedulerPolicy, ScriptConfig,
    WaitKind, ZeroSleep,
};

const PERSIST: i64 = 1 << 20;
const GAME_TIME: i64 = 1 << 14;
const GRANTS: [&str; 13] = [
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
    "Keep",
    "ThreadsAdopt",
];

const DEFS: &[u8] = br#"
Log = {}
function note(...)
    local parts = {}
    for i = 1, select('#', ...) do parts[i] = tostring((select(i, ...))) end
    Log[#Log + 1] = table.concat(parts, ' ')
end
function Sleeper(n)
    Sleep(30)
    note('sleeper', n, GameTime())
end
function Waiter(n)
    local woken = WaitWakeup(40)
    note('wait', n, woken, RealTime())
end
function Forever(n)
    local woken, extra = WaitWakeup()
    note('forever', n, woken, extra)
end
function Msg(n)
    local woken = WaitMsgRaw(10)
    note('msg', n, woken, RealTime())
end
function LoadThreads(list)
    for _, entry in ipairs(list) do
        ThreadsAdopt(Parked(entry[1]), entry[2], entry[3], entry[4], entry[5])
    end
end
"#;

fn policy() -> SchedulerPolicy {
    let on = |native: &str, clock: &str| ClockNative {
        native: native.into(),
        clock: clock.into(),
    };
    let wait = |native: &str| WaitKind {
        native: native.into(),
        timeout_value: ResumeValue::Boolean(false),
        on_resume: None,
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
    }
}

type Handles = Arc<Mutex<Vec<Thread>>>;

/// A game with the original engine's scheduler. `Keep(thread)` collects threads and
/// `Parked(i)` hands out the ones the host put in `handles`.
fn game(handles: &Handles) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    let kept = Arc::clone(handles);
    scripts
        .register_native("Keep", move |_, thread: Thread| {
            kept.lock().unwrap().push(thread);
            Ok(())
        })
        .unwrap();
    let parked = Arc::clone(handles);
    scripts
        .register_native("Parked", move |_, index: usize| {
            Ok(parked.lock().unwrap()[index - 1].clone())
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
    scripts.register_scheduler_with(&names, &policy()).unwrap();
    scripts.register_adopt_native("ThreadsAdopt").unwrap();
    for (kind, native) in [
        ("sleep", "Sleep"),
        ("wakeup", "WaitWakeup"),
        ("msg", "WaitMsgRaw"),
    ] {
        scripts.register_park_native(kind, native).unwrap();
    }
    let grants: Vec<&str> = GRANTS
        .iter()
        .copied()
        .chain(["WakeRaw", "Parked"])
        .collect();
    scripts.add_source("game", &grants).unwrap();
    scripts.run("game", "defs.lua", DEFS).unwrap();
    scripts
}

fn log(scripts: &mut GameScripts) -> Vec<String> {
    let value = scripts.eval_json("game", "return Log").unwrap();
    serde_json::from_value(value).unwrap_or_default()
}

/// The original game: five threads, each stopped in its first wait after 5 ms.
fn original() -> (GameScripts, Handles) {
    let handles = Handles::default();
    let mut scripts = game(&handles);
    scripts
        .run(
            "game",
            "start.lua",
            b"Keep(CreateGameTimeThread(Sleeper, 1))
              Keep(CreateRealTimeThread(Waiter, 2))
              Keep(CreateRealTimeThread(Forever, 3))
              Keep(CreateRealTimeThread(Msg, 4))
              Keep(CreateGameTimeThread(Sleeper, 5))",
        )
        .unwrap();
    assert!(scripts.advance(5).unwrap().is_empty());
    for thread in handles.lock().unwrap().iter() {
        scripts.set_thread_flags(thread, PERSIST);
    }
    (scripts, handles)
}

fn save_flagged(scripts: &GameScripts) -> LuaSnapshot {
    let options = SaveOptions {
        persisted: Some(PERSIST),
    };
    scripts.save_with(&[], &[], &options).unwrap()
}

/// Replaces the engine's wait function frame on top of each thread with a pending call to the
/// native it waits in, which is what an import of an original save holds.
fn as_imported(snapshot: &Snapshot, natives: &[&str]) -> Snapshot {
    let mut imported = snapshot.clone();
    for (data, native) in imported.threads.iter_mut().zip(natives) {
        let ThreadData::Suspended { frames } = data else {
            panic!("a suspended thread");
        };
        frames.pop();
        frames.last_mut().unwrap().pending = Callee::Native(format!("native {native}"));
    }
    imported.captured = (0..natives.len()).collect();
    imported
}

/// A fresh game with the original's threads restored from the imported form, not adopted.
fn restored(natives: &[&str]) -> (GameScripts, Handles, Vec<Thread>) {
    let (old, _) = original();
    let saved = save_flagged(&old);
    let handles = Handles::default();
    let scripts = game(&handles);
    let known = scripts.permanents(&PermanentSpec::default()).unwrap();
    let snapshot = as_imported(&saved.graph, natives);
    let restored = scripts.restore_imported(&snapshot, &known).unwrap();
    handles.lock().unwrap().extend(restored.threads.clone());
    scripts.set_clock("real", 5).unwrap();
    scripts.set_clock("game", 5).unwrap();
    (scripts, handles, restored.threads)
}

/// The natives the saved threads wait in. A save lists them by clock and wake time: the real
/// clock's message wait (thread 4), timed wait (2) and untimed wait (3), then the game clock's
/// sleepers (1 and 5).
const NATIVES: [&str; 5] = ["WaitMsgRaw", "WaitWakeup", "WaitWakeup", "Sleep", "Sleep"];

fn adoption(clock: &str, wait: AdoptWait) -> Adoption {
    Adoption {
        clock: clock.into(),
        wait,
        flags: PERSIST,
        source: "test".into(),
    }
}

fn waiting(kind: &str, timeout: Option<u64>) -> AdoptWait {
    AdoptWait::Wait {
        kind: kind.into(),
        timeout,
    }
}

/// Adopts the five threads of `restored` with the times left at the save, in the saved order.
fn adopt_all(scripts: &GameScripts, threads: &[Thread]) {
    let sleep = |ms| AdoptWait::Sleep(ms);
    let all = [
        ("real", waiting("WaitMsgRaw", Some(10))),
        ("real", waiting("WaitWakeup", Some(35))),
        ("real", waiting("WaitWakeup", None)),
        ("game", sleep(25)),
        ("game", sleep(25)),
    ];
    for (thread, (clock, wait)) in threads.iter().zip(all) {
        scripts.adopt(thread, adoption(clock, wait)).unwrap();
    }
}

#[test]
fn a_restore_notes_the_park_kind_of_each_thread_from_its_top_frame() {
    let (scripts, _, threads) = restored(&NATIVES);
    let kinds: Vec<_> = threads.iter().map(|t| scripts.parked_kind(t)).collect();
    assert_eq!(
        kinds,
        ["msg", "wakeup", "wakeup", "sleep", "sleep"].map(|kind| Some(kind.to_owned()))
    );
    assert_eq!(
        scripts.thread_count(),
        0,
        "a restore alone schedules nothing"
    );
}

#[test]
fn a_native_without_a_park_kind_is_not_noted() {
    let (old, _) = original();
    let saved = save_flagged(&old);
    let handles = Handles::default();
    let scripts = game(&handles);
    let known = scripts.permanents(&PermanentSpec::default()).unwrap();
    let snapshot = as_imported(&saved.graph, &NATIVES);
    let restored = scripts.restore_imported(&snapshot, &known).unwrap();
    scripts
        .adopt(&restored.threads[0], adoption("game", AdoptWait::Sleep(1)))
        .unwrap();
    assert_eq!(
        scripts.parked_kind(&restored.threads[0]),
        None,
        "adoption consumes the note"
    );
}

#[test]
fn adopted_threads_wake_at_their_remaining_times_and_resume_with_the_wait_values() {
    let (mut scripts, _, threads) = restored(&NATIVES);
    adopt_all(&scripts, &threads);
    assert_eq!(scripts.thread_count(), 5);

    assert!(scripts.advance(100).unwrap().is_empty());

    // The message wait times out at real 15, the sleepers wake at game 30 in the order they
    // were adopted, and the other wait times out at real 40 with false. Both clocks run on.
    assert_eq!(
        log(&mut scripts),
        [
            "msg 4 false 15",
            "sleeper 1 30",
            "sleeper 5 30",
            "wait 2 false 40",
        ]
    );
    assert_eq!(scripts.clock_now("real"), Some(105));
    assert_eq!(
        scripts.thread_count(),
        1,
        "the wait without a timeout remains"
    );

    assert!(scripts.wake_thread(&threads[2], vec![Value::Boolean(true), Value::Integer(7)]));
    scripts.advance(1).unwrap();
    assert_eq!(
        log(&mut scripts).last().unwrap(),
        "forever 3 true 7",
        "a wake resumes with true and its values"
    );
}

#[test]
fn threads_due_together_run_in_the_order_they_were_adopted() {
    let (mut scripts, _, threads) = restored(&NATIVES);
    for index in [4, 3] {
        scripts
            .adopt(&threads[index], adoption("game", AdoptWait::Sleep(10)))
            .unwrap();
    }
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["sleeper 5 15", "sleeper 1 15"]);
}

#[test]
fn a_forever_sleeper_waits_until_it_is_woken() {
    let (mut scripts, _, threads) = restored(&NATIVES);
    scripts
        .adopt(&threads[3], adoption("game", AdoptWait::Forever))
        .unwrap();
    scripts.advance(1000).unwrap();
    assert!(log(&mut scripts).is_empty());
    assert_eq!(scripts.thread_count(), 1);
    scripts
        .run("game", "wake.lua", b"WakeSleeper(Parked(4))")
        .unwrap();
    scripts.advance(0).unwrap();
    assert_eq!(log(&mut scripts), ["sleeper 1 1005"]);
}

#[test]
fn the_game_adopts_its_threads_from_lua_with_flags_and_remaining_time() {
    let (mut scripts, _, threads) = restored(&NATIVES);
    assert_eq!(threads.len(), 5);
    scripts
        .run(
            "game",
            "load.lua",
            b"LoadThreads({
                {1, 'real', 16384, 'msg', 10},
                {2, 'real', 16384, nil, 35},
                {4, 'game', 16384, 'timer', 25},
              })",
        )
        .unwrap();
    assert_eq!(scripts.thread_flags(&threads[3]), Some(GAME_TIME));
    assert_eq!(scripts.thread_count(), 3);
    scripts.advance(100).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["msg 4 false 15", "sleeper 1 30", "wait 2 false 40"]
    );
}

#[test]
fn adopting_a_thread_that_was_not_noted_as_parked_fails_in_lua() {
    let (mut scripts, _, _) = restored(&NATIVES);
    let load = "LoadThreads({{1, 'game', 0, nil, 1}}) LoadThreads({{1, 'game', 0, nil, 1}})";
    assert!(scripts.run("game", "twice.lua", load.as_bytes()).is_err());
}

#[test]
fn persisted_threads_are_the_flagged_ones_with_their_remaining_time() {
    let (scripts, handles) = original();
    let threads = handles.lock().unwrap().clone();
    scripts.set_thread_flags(&threads[0], PERSIST | GAME_TIME);
    scripts.set_thread_flags(&threads[1], PERSIST);
    scripts.set_thread_flags(&threads[3], GAME_TIME);
    for index in [2, 4] {
        scripts.set_thread_flags(&threads[index], 0);
    }
    let persisted = scripts.persisted(PERSIST);
    let summary: Vec<_> = persisted
        .iter()
        .map(|p| (p.clock.as_str(), p.remaining, p.park.as_deref()))
        .collect();
    assert_eq!(
        summary,
        [
            ("game", Some(25), Some("sleep")),
            ("real", Some(35), Some("wakeup")),
        ]
    );
}

#[test]
fn a_save_keeps_only_the_persisted_threads_and_a_restore_notes_their_park_kinds() {
    let (old, handles) = original();
    let threads = handles.lock().unwrap().clone();
    old.set_thread_flags(&threads[1], PERSIST);
    old.set_thread_flags(&threads[3], PERSIST);
    old.set_thread_flags(&threads[0], GAME_TIME);

    for index in [2, 4] {
        old.set_thread_flags(&threads[index], 0);
    }

    assert!(
        old.save(&[], &[]).is_err(),
        "without a choice of threads, a save refuses waits"
    );
    let options = SaveOptions {
        persisted: Some(PERSIST),
    };
    let kept = old.save_with(&[], &[], &options).unwrap();
    assert_eq!(kept.threads.len(), 2);

    let mut fresh = game(&Handles::default());
    fresh.restore(&kept, &[]).unwrap();
    assert_eq!(fresh.thread_count(), 2);
    let again = fresh.persisted(PERSIST);
    let kinds: Vec<_> = again
        .iter()
        .map(|p| (fresh.parked_kind(&p.thread), p.flags))
        .collect();
    assert_eq!(
        kinds,
        [
            (Some("msg".to_owned()), PERSIST),
            (Some("wakeup".to_owned()), PERSIST)
        ]
    );
    fresh.advance(100).unwrap();
    assert_eq!(log(&mut fresh), ["msg 4 false 10", "wait 2 false 40"]);
}
