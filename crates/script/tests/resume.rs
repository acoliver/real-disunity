//! Threads resume in the middle of their functions after a save and restore (#147).

use worldrunner_script::{GameScripts, LuaSnapshot, SchedulerNames, ScriptConfig};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

/// A thread that sleeps inside a nested function, inside a `while` loop, inside an `ipairs`
/// loop, so a save can stop it in several kinds of frame.
const GAME: &str = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
function Wait(n) Sleep(n) Note("waited " .. n) end
function Inner(tag)
    local total = 0
    local i = 1
    while i <= 3 do
        Wait(10 * i)
        total = total + i
        Note(tag .. " total " .. total)
        i = i + 1
    end
    return total
end
function Main(tag)
    local sum = 0
    for _, name in ipairs({ "x", "y" }) do
        sum = sum + Inner(tag .. name)
        Sleep(5)
    end
    Note("sum " .. sum)
end
"#;

const ROOTS: [&str; 1] = ["Log"];
const PERMANENTS: [&str; 3] = ["Main", "Inner", "Wait"];

fn game(script: &str) -> GameScripts {
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
    scripts.run("game", "game.lua", script.as_bytes()).unwrap();
    scripts
}

fn log(scripts: &mut GameScripts) -> serde_json::Value {
    scripts.eval_json("game", "return Log").unwrap()
}

fn through_json(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

fn started() -> GameScripts {
    let mut scripts = game(GAME);
    scripts
        .run("game", "start.lua", b"CreateGameTimeThread(Main, 't')")
        .unwrap();
    scripts
}

/// Saves `scripts` and restores the save into a fresh state that ran the same scripts.
fn reload(scripts: &GameScripts) -> GameScripts {
    let snapshot = through_json(&scripts.save(&ROOTS, &PERMANENTS).unwrap());
    let mut fresh = game(GAME);
    fresh.restore(&snapshot, &PERMANENTS).unwrap();
    fresh
}

fn run_to_end(scripts: &mut GameScripts) {
    for _ in 0..30 {
        assert_eq!(scripts.advance(5).unwrap(), []);
    }
}

#[test]
fn a_thread_saved_at_every_wake_point_continues_as_the_uninterrupted_run() {
    let mut uninterrupted = started();
    run_to_end(&mut uninterrupted);
    let expected = log(&mut uninterrupted);
    assert_eq!(expected.as_array().unwrap().len(), 13);

    let stepping = started();
    for step in 0..30 {
        assert_eq!(stepping.advance(5).unwrap(), []);
        let mut resumed = reload(&stepping);
        for _ in step + 1..30 {
            assert_eq!(resumed.advance(5).unwrap(), []);
        }
        assert_eq!(log(&mut resumed), expected, "saved after step {step}");
    }
}

#[test]
fn a_restored_thread_can_be_saved_and_restored_again() {
    let mut uninterrupted = started();
    run_to_end(&mut uninterrupted);
    let expected = log(&mut uninterrupted);

    let mut scripts = started();
    for _ in 0..30 {
        assert_eq!(scripts.advance(5).unwrap(), []);
        scripts = reload(&scripts);
    }
    assert_eq!(log(&mut scripts), expected);
}

#[test]
fn functions_made_by_load_resume_too() {
    let script = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
local make = load([[
    function Slow(tag)
        Sleep(10)
        Note(tag .. " middle")
        Sleep(10)
        Note(tag .. " end")
    end
]], "=slow", "t", _ENV)
make()
"#;
    let mut scripts = game(script);
    scripts
        .run("game", "start.lua", b"CreateGameTimeThread(Slow, 's')")
        .unwrap();
    scripts.advance(0).unwrap();
    scripts.advance(10).unwrap();
    let snapshot = through_json(&scripts.save(&ROOTS, &["Slow"]).unwrap());
    let mut fresh = game(script);
    fresh.restore(&snapshot, &["Slow"]).unwrap();
    fresh.advance(10).unwrap();
    assert_eq!(
        log(&mut fresh),
        serde_json::json!(["s middle@10", "s end@20"])
    );
}

#[test]
fn scripts_cannot_reach_the_debug_library() {
    let mut scripts = game(GAME);
    let reachable = scripts
        .eval_json(
            "game",
            r#"return {
                debug == nil,
                _G.debug == nil,
                load("return debug")() == nil,
                coroutine.debug == nil,
                string.debug == nil,
            }"#,
        )
        .unwrap();
    assert_eq!(reachable, serde_json::json!([true, true, true, true, true]));
}

#[test]
fn a_chunk_the_rewrite_refuses_fails_to_load_with_the_reason() {
    let mut scripts = game(GAME);
    let error = scripts
        .run("game", "bad.lua", b"local __lrs = 1")
        .unwrap_err()
        .to_string();
    assert!(error.contains("bad.lua"), "{error}");
    assert!(error.contains("cannot be made resumable"), "{error}");

    let message = scripts
        .eval_json(
            "game",
            r#"local f, message = load("local __lrs = 1", "=bad") return { f == nil, message }"#,
        )
        .unwrap();
    assert_eq!(message[0], serde_json::json!(true));
    assert!(message[1].as_str().unwrap().contains("=bad"), "{message}");
}

#[test]
fn statement_level_assert_and_dbg_calls_can_be_stripped_at_load() {
    let mut scripts = game(GAME);
    scripts.strip_debug_calls(true);
    scripts
        .run(
            "game",
            "strip.lua",
            b"assert(false, 'gone') dbg('gone') Kept = 1",
        )
        .unwrap();
    assert_eq!(
        scripts.eval_json("game", "return Kept").unwrap(),
        serde_json::json!(1)
    );
    let mut strict = game(GAME);
    let error = strict
        .run("game", "strip.lua", b"assert(false, 'kept')")
        .unwrap_err()
        .to_string();
    assert!(error.contains("kept"), "{error}");
}
