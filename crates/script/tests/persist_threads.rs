//! Scheduler threads saved where they stopped and continued on restore (#147).

use worldrunner_script::{
    GameScripts, LuaSnapshot, SaveOptions, SavedWake, SchedulerNames, SchedulerPolicy,
    ScriptConfig, ThreadSchedule,
};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

/// A game whose units keep their progress in tables and run a patrol thread each.
const GAME: &str = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
Units = {}
function Patrol(unit, period)
    while true do
        Sleep(period)
        unit.steps = unit.steps + 1
        Note(unit.name .. unit.steps)
    end
end
function Guard(unit) Sleep(math.huge) Note(unit.name .. " alerted") end
function Spawn(name, period)
    local unit = { name = name, steps = 0 }
    Units[#Units + 1] = unit
    unit.thread = CreateGameTimeThread(Patrol, unit, period)
    return unit
end
"#;

fn game() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_scheduler(
            &SchedulerNames {
                spawn: NAMES[0].into(),
                sleep: NAMES[1].into(),
                wakeup: NAMES[2].into(),
                kill: NAMES[3].into(),
                current: NAMES[4].into(),
                now: NAMES[5].into(),
            },
            &SchedulerPolicy::default(),
        )
        .unwrap();
    scripts.add_source("game", &NAMES).unwrap();
    scripts.run("game", "game.lua", GAME.as_bytes()).unwrap();
    scripts
}

fn json(scripts: &mut GameScripts, code: &str) -> serde_json::Value {
    scripts.eval_json("game", code).unwrap()
}

fn round_trip(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

#[test]
fn threads_continue_where_they_stopped_at_their_wake_time() {
    let mut saved = game();
    saved
        .run(
            "game",
            "start.lua",
            b"Spawn('a', 30) Spawn('b', 50) Sentry = { name = 's' }
              Sentry.thread = CreateGameTimeThread(Guard, Sentry)
              Done = CreateGameTimeThread(function() end)",
        )
        .unwrap();
    saved.advance(70).unwrap();
    assert_eq!(
        json(&mut saved, "return Log"),
        serde_json::json!(["a1@30", "b1@50", "a2@60"])
    );
    let snapshot = round_trip(
        &saved
            .save(
                &["Units", "Sentry", "Done"],
                &["Patrol", "Guard"],
                &SaveOptions::default(),
            )
            .unwrap(),
    );
    assert_eq!(snapshot.clocks, [("game".to_owned(), 70)]);
    let wakes: Vec<SavedWake> = snapshot.threads.iter().map(|t| t.wake).collect();
    assert_eq!(
        wakes,
        [SavedWake::At(90), SavedWake::At(100), SavedWake::UntilWoken,]
    );
    let unstarted: Vec<bool> = snapshot.threads.iter().map(|t| t.start.is_some()).collect();
    assert_eq!(unstarted, [false, false, false]);

    let mut restored = game();
    restored.restore(&snapshot, &["Patrol", "Guard"]).unwrap();
    assert_eq!(restored.now(), 70);
    assert_eq!(restored.thread_count(), 3);
    json(
        &mut restored,
        "Wakeup(Sentry.thread) DeleteThread(Units[2].thread)",
    );
    let failures = restored.advance(60).unwrap();
    assert_eq!(failures, []);
    // Patrol was asleep inside its loop until 90, so it steps then without sleeping its period
    // again first. Guard was asleep inside the function, so the wakeup finishes it.
    assert_eq!(
        json(&mut restored, "return Log"),
        serde_json::json!(["s alerted@70", "a3@90", "a4@120"])
    );
    assert_eq!(
        json(
            &mut restored,
            "return { coroutine.status(Units[1].thread), coroutine.status(Sentry.thread), coroutine.status(Done) }"
        ),
        serde_json::json!(["suspended", "dead", "dead"])
    );
}

#[test]
fn snapshots_that_do_not_fit_the_state_are_refused() {
    let mut saved = game();
    saved.run("game", "a.lua", b"Spawn('a', 10)").unwrap();
    let snapshot = saved
        .save(&["Units"], &["Patrol"], &SaveOptions::default())
        .unwrap();
    let mut busy = game();
    busy.run("game", "b.lua", b"Spawn('b', 10)").unwrap();
    let error = busy
        .restore(&snapshot, &["Patrol"])
        .unwrap_err()
        .to_string();
    assert!(error.contains("already has threads"), "{error}");
    let renamed = game()
        .restore(&snapshot, &["Guard"])
        .unwrap_err()
        .to_string();
    assert!(renamed.contains("Patrol"), "{renamed}");
}

#[test]
fn a_snapshot_without_its_graph_or_with_unmatched_schedules_is_refused_by_name() {
    let mut saved = game();
    saved
        .run("game", "a.lua", b"Spawn('a', 30) Spawn('b', 50)")
        .unwrap();
    let snapshot = saved
        .save(&["Units"], &["Patrol"], &SaveOptions::default())
        .unwrap();
    assert_eq!((snapshot.threads.len(), snapshot.schedules.len()), (2, 2));

    // A snapshot from before the graph has no `graph` field and is not read at all.
    let mut encoded = serde_json::to_value(&snapshot).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .remove("graph")
        .expect("a graph field");
    let missing = serde_json::from_value::<LuaSnapshot>(encoded)
        .unwrap_err()
        .to_string();
    assert!(missing.contains("missing field `graph`"), "{missing}");

    // Each thread needs its schedule: neither a thread without one nor a schedule without a
    // thread is dropped silently.
    let mut short = snapshot.clone();
    short.schedules.pop();
    let mut long = snapshot.clone();
    long.schedules.push(ThreadSchedule::default());
    for (unmatched, expected) in [
        (short, "the snapshot has 2 threads and 1 schedules"),
        (long, "the snapshot has 2 threads and 3 schedules"),
    ] {
        let mut restored = game();
        let error = restored
            .restore(&unmatched, &["Patrol"])
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
        assert_eq!(restored.thread_count(), 0, "nothing was scheduled");
        assert_eq!(json(&mut restored, "return #Units"), serde_json::json!(0));
    }
}

#[test]
fn data_only_restores_leave_running_threads_alone() {
    let mut saved = game();
    saved.run("game", "data.lua", b"Score = { 3 }").unwrap();
    let snapshot = saved
        .save(&["Score"], &["Patrol"], &SaveOptions::default())
        .unwrap();
    assert!(snapshot.threads.is_empty());
    let mut running = game();
    running.run("game", "a.lua", b"Spawn('a', 10)").unwrap();
    running.advance(25).unwrap();
    running.restore(&snapshot, &["Patrol"]).unwrap();
    assert_eq!(
        running.now(),
        25,
        "the clock is untouched without saved threads"
    );
    assert_eq!(running.thread_count(), 1);
    assert_eq!(json(&mut running, "return Score[1]"), serde_json::json!(3));
}
