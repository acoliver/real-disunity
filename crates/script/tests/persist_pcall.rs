//! Scheduler threads saved while suspended inside `pcall` and `xpcall` (#147).

use worldrunner_script::{GameScripts, LuaSnapshot, SchedulerNames, ScriptConfig};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

/// A worker that sleeps inside a protected call and then fails, and a worker that does the same
/// under `xpcall` with a message handler.
const GAME: &str = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
function Risky(label)
    Sleep(50)
    error(label .. " failed")
end
function Worker(label)
    local ok, err = pcall(Risky, label)
    Note(label .. " ok=" .. tostring(ok) .. " " .. tostring(err):match("%a+ failed$"))
    Sleep(10)
    Note(label .. " done")
end
function Handler(err) return "handled: " .. tostring(err):match("%a+ failed$") end
function GuardedWorker(label)
    local ok, err = xpcall(Risky, Handler, label)
    Note(label .. " ok=" .. tostring(ok) .. " " .. err)
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

fn round_trip(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

fn log(scripts: &mut GameScripts) -> serde_json::Value {
    scripts.eval_json("game", "return Log").unwrap()
}

const PERMANENTS: [&str; 3] = ["Worker", "GuardedWorker", "Risky"];

#[test]
fn a_thread_suspended_in_pcall_catches_the_error_after_restore() {
    let mut uninterrupted = game();
    uninterrupted
        .run("game", "start.lua", b"CreateGameTimeThread(Worker, 'a')")
        .unwrap();
    assert_eq!(uninterrupted.advance(100).unwrap(), []);
    let expected = log(&mut uninterrupted);
    assert_eq!(
        expected,
        serde_json::json!(["a ok=false a failed@50", "a done@60"])
    );

    let mut saved = game();
    saved
        .run("game", "start.lua", b"CreateGameTimeThread(Worker, 'a')")
        .unwrap();
    saved.advance(20).unwrap();
    assert_eq!(
        saved.eval_json("game", "return #Log").unwrap(),
        serde_json::json!(0)
    );
    let snapshot = round_trip(&saved.save(&[], &PERMANENTS).unwrap());

    let mut restored = game();
    restored.restore(&snapshot, &PERMANENTS).unwrap();
    assert_eq!(restored.now(), 20);
    assert_eq!(restored.thread_count(), 1);
    assert_eq!(restored.advance(80).unwrap(), []);
    assert_eq!(log(&mut restored), expected);
}

#[test]
fn a_thread_suspended_in_xpcall_runs_the_handler_after_restore() {
    let mut saved = game();
    saved
        .run(
            "game",
            "start.lua",
            b"CreateGameTimeThread(GuardedWorker, 'g')",
        )
        .unwrap();
    saved.advance(20).unwrap();
    let snapshot = round_trip(&saved.save(&[], &PERMANENTS).unwrap());

    let mut restored = game();
    restored.restore(&snapshot, &PERMANENTS).unwrap();
    assert_eq!(restored.advance(80).unwrap(), []);
    assert_eq!(
        log(&mut restored),
        serde_json::json!(["g ok=false handled: g failed@50"])
    );
}

#[test]
fn a_restored_pcall_thread_can_be_saved_again() {
    let mut first = game();
    first
        .run("game", "start.lua", b"CreateGameTimeThread(Worker, 'a')")
        .unwrap();
    first.advance(20).unwrap();
    let snapshot = round_trip(&first.save(&[], &PERMANENTS).unwrap());

    let mut second = game();
    second.restore(&snapshot, &PERMANENTS).unwrap();
    second.advance(10).unwrap();
    let again = round_trip(&second.save(&[], &PERMANENTS).unwrap());

    let mut third = game();
    third.restore(&again, &PERMANENTS).unwrap();
    assert_eq!(third.now(), 30);
    assert_eq!(third.advance(70).unwrap(), []);
    assert_eq!(
        log(&mut third),
        serde_json::json!(["a ok=false a failed@50", "a done@60"])
    );
}
