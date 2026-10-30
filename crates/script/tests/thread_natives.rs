//! Natives a game binds for thread flags, sources, clocks and interrupting an advance (#326).
//!
//! The semantics follow the cthreads natives of rd-stranded: `ThreadSetFlags` adds bits,
//! `ThreadHasFlags` needs every bit of the mask, a clock set from Lua keeps the wake times of
//! waiting threads, and `InterruptAdvance` ends the advance after the running thread.

use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use worldrunner_script::{
    ClockNative, FlagNatives, FlaggedSpawn, GameScripts, SaveOptions, SchedulerNames,
    SchedulerPolicy, ScriptConfig,
};

const PERSIST: i64 = 1 << 20;
const REAL: i64 = 1 << 13;
const GAME: i64 = 1 << 14;
const ON_MAP: i64 = 1 << 21;
const GRANTS: [&str; 15] = [
    "CreateRealTimeThread",
    "CreateGameTimeThread",
    "CreateMapRealTimeThread",
    "Sleep",
    "now",
    "GetFlags",
    "ThreadSetFlags",
    "ThreadHasFlags",
    "ThreadsSetThreadSource",
    "ThreadClock",
    "ResetGameTime",
    "ResetRealTime",
    "InterruptAdvance",
    "GameTime",
    "CurrentThread",
];

fn names() -> SchedulerNames {
    SchedulerNames {
        spawn: "CreateRealTimeThread".into(),
        sleep: "Sleep".into(),
        wakeup: "Wakeup".into(),
        kill: "DeleteThread".into(),
        current: "CurrentThread".into(),
        now: "now".into(),
    }
}

fn policy() -> SchedulerPolicy {
    let flagged = |native: &str, clock: &str, flags| FlaggedSpawn {
        native: native.into(),
        clock: clock.into(),
        flags,
    };
    let on = |native: &str, clock: &str| ClockNative {
        native: native.into(),
        clock: clock.into(),
    };
    SchedulerPolicy {
        clocks: vec!["real".into(), "game".into()],
        spawn_flagged: vec![
            flagged("CreateGameTimeThread", "game", GAME),
            flagged("CreateMapRealTimeThread", "real", REAL | ON_MAP),
        ],
        read_clock: vec![on("GameTime", "game")],
        thread_flags: Some(FlagNatives {
            get: Some("GetFlags".into()),
            set: Some("ThreadSetFlags".into()),
            has: Some("ThreadHasFlags".into()),
        }),
        set_source: Some("ThreadsSetThreadSource".into()),
        clock_of: Some("ThreadClock".into()),
        set_clock: vec![on("ResetGameTime", "game"), on("ResetRealTime", "real")],
        interrupt: Some("InterruptAdvance".into()),
        ..SchedulerPolicy::default()
    }
}

/// The natives are registered with the scheduler, before the first source is added.
fn game() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_scheduler_with(&names(), &policy())
        .unwrap();
    scripts.add_source("game", &GRANTS).unwrap();
    scripts
        .run(
            "game",
            "log.lua",
            b"Log = {} function Note(s) Log[#Log + 1] = s .. '@' .. now() end",
        )
        .unwrap();
    scripts
}

fn run(scripts: &mut GameScripts, code: &str) {
    static FILES: AtomicUsize = AtomicUsize::new(0);
    let file = format!("test{}.lua", FILES.fetch_add(1, Ordering::SeqCst));
    scripts.run("game", &file, code.as_bytes()).unwrap();
}

fn log(scripts: &mut GameScripts) -> Vec<String> {
    let value = scripts.eval_json("game", "return Log").unwrap();
    serde_json::from_value(value).unwrap_or_default()
}

fn eval(scripts: &mut GameScripts, code: &str) -> serde_json::Value {
    scripts.eval_json("game", code).unwrap()
}

#[test]
fn set_flags_adds_bits_and_has_flags_needs_every_bit_of_the_mask() {
    let mut scripts = game();
    run(
        &mut scripts,
        "T = CreateRealTimeThread(function() Sleep(10) end)",
    );
    let result = eval(
        &mut scripts,
        "local r = {GetFlags(T), ThreadHasFlags(T, 0)}
         ThreadSetFlags(T, 4)
         ThreadSetFlags(T, 2)
         r[3] = GetFlags(T)
         r[4] = ThreadHasFlags(T, 6)
         r[5] = ThreadHasFlags(T, 2)
         r[6] = ThreadHasFlags(T, 10)
         r[7] = ThreadHasFlags(T, 8)
         return r",
    );
    assert_eq!(result, json!([0, true, 6, true, true, false, false]));
}

#[test]
fn flags_natives_treat_values_that_are_not_scheduler_threads_as_unflagged() {
    let mut scripts = game();
    let result = eval(
        &mut scripts,
        "local co = coroutine.create(function() end)
         ThreadSetFlags(co, 4)
         ThreadSetFlags(nil, 4)
         return {GetFlags(co) == nil, ThreadHasFlags(co, 0), ThreadHasFlags(nil, 0),
                 ThreadClock(co) == nil, ThreadClock(5) == nil}",
    );
    assert_eq!(result, json!([true, false, false, true, true]));
}

#[test]
fn a_flagged_spawn_creates_the_thread_with_its_flags_on_its_clock() {
    let mut scripts = game();
    run(
        &mut scripts,
        "G = CreateGameTimeThread(function() Sleep(5) Note('game') end)
         M = CreateMapRealTimeThread(function() Sleep(5) Note('map') end)
         R = CreateRealTimeThread(function() Sleep(5) Note('real') end)",
    );
    let result = eval(
        &mut scripts,
        "return {GetFlags(G), GetFlags(M), GetFlags(R),
                ThreadClock(G), ThreadClock(M), ThreadClock(R)}",
    );
    assert_eq!(
        result,
        json!([GAME, REAL | ON_MAP, 0, "game", "real", "real"])
    );
    scripts.advance(5).unwrap();
    let mut ran = log(&mut scripts);
    ran.sort();
    assert_eq!(ran, ["game@5", "map@5", "real@5"]);
    assert_eq!(scripts.clock_now("game"), Some(5));
}

#[test]
fn the_clock_of_a_thread_follows_it_through_its_states() {
    let mut scripts = game();
    run(
        &mut scripts,
        "G = CreateGameTimeThread(function()
             Note(ThreadClock(CurrentThread()))
             Sleep(5)
         end)
         Before = ThreadClock(G)",
    );
    scripts.advance(1).unwrap();
    assert_eq!(log(&mut scripts), ["game@0"]);
    let result = eval(&mut scripts, "return {Before, ThreadClock(G)}");
    assert_eq!(result, json!(["game", "game"]), "queued and then sleeping");
}

#[test]
fn a_source_set_from_lua_is_kept_by_persisted_and_by_a_save() {
    let mut scripts = game();
    run(
        &mut scripts,
        "A = CreateGameTimeThread(function() Sleep(30) end)
         B = CreateMapRealTimeThread(function() Sleep(40) end)
         C = CreateGameTimeThread(function() Sleep(50) end)
         ThreadSetFlags(A, 1 << 20)
         ThreadSetFlags(B, 1 << 20)
         ThreadsSetThreadSource(A, 'Cat.lua')
         ThreadsSetThreadSource(B, 'Map.lua')
         ThreadsSetThreadSource(C, 'Skipped.lua')",
    );
    scripts.advance(10).unwrap();

    let persisted = scripts.persisted(PERSIST);
    let summary: Vec<_> = persisted
        .iter()
        .map(|p| (p.source.as_str(), p.clock.as_str(), p.flags, p.remaining))
        .collect();
    assert_eq!(
        summary,
        [
            ("Cat.lua", "game", GAME | PERSIST, Some(20)),
            ("Map.lua", "real", REAL | ON_MAP | PERSIST, Some(30)),
        ]
    );

    let options = SaveOptions {
        persisted: Some(PERSIST),
    };
    let saved = scripts.save_with(&[], &[], &options).unwrap();
    let schedules: Vec<_> = saved
        .schedules
        .iter()
        .map(|s| (s.source.clone(), s.clock.clone(), s.flags))
        .collect();
    assert_eq!(schedules.len(), 2, "the unflagged thread is not saved");
    assert!(schedules.contains(&(
        "Cat.lua".to_owned(),
        Some("game".to_owned()),
        GAME | PERSIST
    )));
    assert!(schedules.contains(&(
        "Map.lua".to_owned(),
        Some("real".to_owned()),
        REAL | ON_MAP | PERSIST
    )));
}

#[test]
fn a_clock_set_from_lua_moves_when_waiting_threads_are_due() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateGameTimeThread(function() Sleep(100) Note('late') end)
         CreateGameTimeThread(function() Sleep(20) Note('early') end)",
    );
    scripts.advance(10).unwrap();
    assert_eq!(scripts.clock_now("game"), Some(10));
    assert!(log(&mut scripts).is_empty());

    run(&mut scripts, "ResetGameTime(90)");
    assert_eq!(scripts.clock_now("game"), Some(90));
    assert_eq!(scripts.clock_now("real"), Some(10), "other clocks stay");
    scripts.advance(0).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["early@90"],
        "a wake time already behind the clock is due at once, and at the clock's time"
    );
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["early@90", "late@100"]);

    run(&mut scripts, "ResetGameTime(0) ResetRealTime(7)");
    assert_eq!(scripts.clock_now("game"), Some(0));
    assert_eq!(scripts.clock_now("real"), Some(7));
    run(&mut scripts, "ResetGameTime()");
    assert_eq!(scripts.clock_now("game"), Some(0), "nil is zero");
}

#[test]
fn a_clock_set_back_makes_waiting_threads_wait_longer() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateGameTimeThread(function() Sleep(50) Note('woke') end)",
    );
    scripts.advance(10).unwrap();
    run(&mut scripts, "ResetGameTime(0)");
    scripts.advance(49).unwrap();
    assert!(log(&mut scripts).is_empty(), "wake time 50 is absolute");
    scripts.advance(1).unwrap();
    assert_eq!(log(&mut scripts), ["woke@50"]);
}

#[test]
fn an_interrupt_ends_the_advance_and_the_other_due_threads_stay_due() {
    let mut scripts = game();
    run(
        &mut scripts,
        "CreateRealTimeThread(function() Sleep(5) Note('a') Sleep(100) end)
         CreateRealTimeThread(function() Sleep(10) Note('b') InterruptAdvance() Sleep(100) end)
         CreateRealTimeThread(function() Sleep(10) Note('c') Sleep(100) end)
         CreateRealTimeThread(function() Sleep(20) Note('d') Sleep(100) end)",
    );
    scripts.advance(50).unwrap();
    assert_eq!(log(&mut scripts), ["a@5", "b@10"]);
    assert_eq!(scripts.now(), 10, "the clock stops where the advance ended");
    assert_eq!(scripts.thread_count(), 4);

    scripts.advance(0).unwrap();
    assert_eq!(log(&mut scripts), ["a@5", "b@10", "c@10"]);
    scripts.advance(10).unwrap();
    assert_eq!(log(&mut scripts), ["a@5", "b@10", "c@10", "d@20"]);
}

#[test]
fn an_interrupt_applies_to_one_advance_only() {
    let mut scripts = game();
    run(
        &mut scripts,
        "InterruptAdvance()
         CreateRealTimeThread(function() Sleep(5) Note('late') end)",
    );
    scripts.advance(10).unwrap();
    assert_eq!(
        log(&mut scripts),
        ["late@5"],
        "set outside an advance, it is dropped"
    );
}
