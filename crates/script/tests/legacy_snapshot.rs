//! Snapshots saved before all state was one graph restore into the current form (#147).

use worldrunner_script::{
    GameScripts, LuaSnapshot, Saved, SavedTable, SavedThread, SavedWake, SchedulerNames,
    ScriptConfig,
};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

const GAME: &str = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
function Patrol(unit, period)
    while true do
        Sleep(period)
        unit.steps = unit.steps + 1
        Note(unit.name .. unit.steps)
    end
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

fn text(s: &str) -> Saved {
    Saved::String(s.as_bytes().to_vec())
}

fn table(entries: Vec<(Saved, Saved)>, metatable: Option<Saved>) -> SavedTable {
    SavedTable { entries, metatable }
}

/// A snapshot in the format that predates the graph: a `Units` list with one unit that has a
/// metatable, a patrol thread that had been started with the unit as its argument, and a thread
/// that had finished.
fn old_snapshot() -> LuaSnapshot {
    LuaSnapshot {
        roots: vec![("Units".into(), Saved::Table(0))],
        tables: vec![
            table(vec![(Saved::Integer(1), Saved::Table(1))], None),
            table(
                vec![
                    (text("name"), text("a")),
                    (text("steps"), Saved::Integer(2)),
                    (text("speed"), Saved::Float(2.5f64.to_bits())),
                    (text("alive"), Saved::Boolean(true)),
                    (text("self"), Saved::Table(1)),
                ],
                Some(Saved::Table(2)),
            ),
            table(vec![(text("__index"), Saved::Table(3))], None),
            table(vec![(text("kind"), text("scout"))], None),
        ],
        threads: vec![
            SavedThread {
                wake: SavedWake::At(90),
                start: None,
                function: Some(Saved::Permanent("Patrol".into())),
                args: vec![Saved::Table(1), Saved::Integer(30)],
            },
            SavedThread {
                wake: SavedWake::Finished,
                start: None,
                function: None,
                args: Vec::new(),
            },
        ],
        clock: 70,
        ..LuaSnapshot::default()
    }
}

fn json(scripts: &mut GameScripts, code: &str) -> serde_json::Value {
    scripts.eval_json("game", code).unwrap()
}

#[test]
fn a_snapshot_from_before_the_graph_restores_its_data_and_threads() {
    let mut restored = game();
    restored.restore(&old_snapshot(), &["Patrol"]).unwrap();
    assert_eq!(restored.now(), 70);
    assert_eq!(restored.thread_count(), 1);
    let unit = json(
        &mut restored,
        "local u = Units[1] return { u.name, u.steps, u.speed, u.alive, u.self == u, u.kind }",
    );
    assert_eq!(unit, serde_json::json!(["a", 2, 2.5, true, true, "scout"]));
    let metatable_kept = json(
        &mut restored,
        "return getmetatable(Units[1]) ~= nil and rawget(Units[1], 'kind') == nil",
    );
    assert_eq!(metatable_kept, serde_json::json!(true));

    // The thread's function runs from its start at the saved wake time.
    assert_eq!(restored.advance(20).unwrap(), []);
    assert_eq!(json(&mut restored, "return Log"), serde_json::json!({}));
    assert_eq!(restored.advance(30).unwrap(), []);
    assert_eq!(
        json(&mut restored, "return Log"),
        serde_json::json!(["a3@120"])
    );
    assert_eq!(
        json(&mut restored, "return Units[1].steps"),
        serde_json::json!(3)
    );
}

#[test]
fn a_snapshot_saved_by_a_development_build_without_a_function_is_refused() {
    let snapshot = LuaSnapshot {
        threads: vec![SavedThread {
            wake: SavedWake::At(5),
            start: None,
            function: Some(Saved::Nil),
            args: Vec::new(),
        }],
        ..LuaSnapshot::default()
    };
    let error = game()
        .restore(&snapshot, &["Patrol"])
        .unwrap_err()
        .to_string();
    assert!(error.contains("development build"), "{error}");
}

#[test]
fn a_metatable_that_is_not_a_table_is_refused() {
    let snapshot = LuaSnapshot {
        roots: vec![("Units".into(), Saved::Table(0))],
        tables: vec![table(Vec::new(), Some(Saved::Integer(1)))],
        ..LuaSnapshot::default()
    };
    let error = game()
        .restore(&snapshot, &["Patrol"])
        .unwrap_err()
        .to_string();
    assert!(error.contains("metatable must be a table"), "{error}");
}
