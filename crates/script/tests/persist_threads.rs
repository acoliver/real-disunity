//! Scheduler threads saved as data and restarted from their function on restore (#147).

use worldrunner_script::{
    GameScripts, LuaSnapshot, Saved, SavedWake, SchedulerNames, ScriptConfig,
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
        .register_scheduler(&SchedulerNames {
            spawn: NAMES[0].into(),
            sleep: NAMES[1].into(),
            wakeup: NAMES[2].into(),
            kill: NAMES[3].into(),
            current: NAMES[4].into(),
            now: NAMES[5].into(),
        })
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
fn threads_restart_from_their_function_at_their_wake_time() {
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
            .save(&["Units", "Sentry", "Done"], &["Patrol", "Guard"])
            .unwrap(),
    );
    assert_eq!(snapshot.clock, 70);
    let wakes: Vec<SavedWake> = snapshot.threads.iter().map(|t| t.wake).collect();
    assert_eq!(
        wakes,
        [
            SavedWake::At(90),
            SavedWake::At(100),
            SavedWake::UntilWoken,
            SavedWake::Finished
        ]
    );
    assert_eq!(
        snapshot.threads[0].function,
        Saved::Permanent("Patrol".into())
    );

    let mut restored = game();
    restored.restore(&snapshot, &["Patrol", "Guard"]).unwrap();
    assert_eq!(restored.now(), 70);
    assert_eq!(restored.thread_count(), 3);
    json(
        &mut restored,
        "Wakeup(Sentry.thread) DeleteThread(Units[2].thread)",
    );
    restored.advance(60).unwrap();
    // Restarted at its wake time, Patrol sleeps its period again before stepping, and the steps
    // continue from the restored table. Guard starts again too, so the wakeup only restarts it
    // and it waits to be woken once more.
    assert_eq!(
        json(&mut restored, "return Log"),
        serde_json::json!(["a3@120"])
    );
    json(&mut restored, "Wakeup(Sentry.thread)");
    restored.advance(0).unwrap();
    assert_eq!(
        json(
            &mut restored,
            "return { Log, coroutine.status(Units[1].thread), coroutine.status(Done) }"
        ),
        serde_json::json!([["a3@120", "s alerted@130"], "suspended", "dead"])
    );
}

#[test]
fn threads_that_cannot_be_saved_or_restored_are_refused() {
    let mut scripts = game();
    scripts
        .run(
            "game",
            "closure.lua",
            b"Ad = CreateGameTimeThread(function() Sleep(5) end)",
        )
        .unwrap();
    let closure = scripts.save(&[], &["Patrol"]).unwrap_err().to_string();
    assert!(
        closure.contains(
            "the function scheduler thread 0 was spawned with is a function that is not a permanent"
        ),
        "{closure}"
    );

    let mut saved = game();
    saved.run("game", "a.lua", b"Spawn('a', 10)").unwrap();
    let snapshot = saved.save(&["Units"], &["Patrol"]).unwrap();
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
fn data_only_restores_leave_running_threads_alone() {
    let mut saved = game();
    saved.run("game", "data.lua", b"Score = { 3 }").unwrap();
    let snapshot = saved.save(&["Score"], &["Patrol"]).unwrap();
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
