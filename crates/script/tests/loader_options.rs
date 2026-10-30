//! Opt-in loader behaviour for scripts written for another engine's Lua: integer division,
//! chunks the rewrite refuses, and chunk names that are reused (#147).

use worldrunner_script::{
    DivisionSemantics, GameScripts, NameCollisions, RefusedChunks, SchedulerNames, ScriptConfig,
};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

fn with_division(division: DivisionSemantics) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts.set_division(division).unwrap();
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
    scripts
}

fn scripts() -> GameScripts {
    with_division(DivisionSemantics::Stock)
}

fn integer_dividing() -> GameScripts {
    with_division(DivisionSemantics::IntegerTruncating)
}

fn eval(scripts: &mut GameScripts, code: &str) -> serde_json::Value {
    scripts.eval_json("game", code).unwrap()
}

#[test]
fn division_is_stock_by_default() {
    let mut scripts = scripts();
    scripts.run("game", "d.lua", b"Q = 7 / 2").unwrap();
    assert_eq!(eval(&mut scripts, "return Q"), serde_json::json!(3.5));
}

#[test]
fn a_game_chunk_divides_integers_as_integers_truncating_toward_zero() {
    let mut scripts = integer_dividing();
    scripts
        .run(
            "game",
            "d.lua",
            b"R = { 7 / 2, -7 / 2, 7 / -2, 6 / 3, 0 / 5, 7.0 / 2, 7 / 2.0, 1.5 / 0.5, '9' / 2 }
              T = { math.type(7 / 2), math.type(7.0 / 2), math.type(1 / 0.0) }",
        )
        .unwrap();
    assert_eq!(
        eval(&mut scripts, "return R"),
        serde_json::json!([3, -3, -3, 2, 0, 3.5, 3.5, 3.0, 4.5])
    );
    assert_eq!(
        eval(&mut scripts, "return T"),
        serde_json::json!(["integer", "float", "float"])
    );
}

#[test]
fn division_by_an_integer_zero_raises_a_lua_error_naming_the_cause() {
    let mut scripts = integer_dividing();
    let caught = scripts
        .run(
            "game",
            "z.lua",
            b"local ok, message = pcall(function() return 1 / 0 end)
              Caught = { ok, message }
              Float = 1 / 0.0",
        )
        .and_then(|()| scripts.eval_json("game", "return Caught"))
        .unwrap();
    assert_eq!(caught[0], serde_json::json!(false));
    let message = caught[1].as_str().unwrap();
    assert!(message.contains("division by zero"), "{message}");
    assert_eq!(
        eval(&mut scripts, "return Float == math.huge"),
        serde_json::json!(true)
    );
}

#[test]
fn text_compiled_with_load_divides_the_same_way() {
    let mut scripts = integer_dividing();
    let value = eval(
        &mut scripts,
        "local f = load('return 7 / 2') return { math.type(f()), f(), load('return 7.0 / 2')() }",
    );
    assert_eq!(value, serde_json::json!(["integer", 3, 3.5]));
}

#[test]
fn a_division_operand_may_be_any_expression_and_metamethods_still_apply() {
    let mut scripts = integer_dividing();
    scripts
        .run(
            "game",
            "ops.lua",
            b"local m = setmetatable({}, { __div = function(a, b) return 'meta' end })
              local function two() return 2 end
              Out = { 9 / two(), m / 3, 3 / m }",
        )
        .unwrap();
    assert_eq!(
        eval(&mut scripts, "return Out"),
        serde_json::json!([4, "meta", "meta"])
    );
}

#[test]
fn code_evaluated_by_the_host_divides_the_same_way() {
    let mut scripts = integer_dividing();
    assert_eq!(eval(&mut scripts, "return 7 / 2"), serde_json::json!(3));
    assert_eq!(eval(&mut scripts, "return 7 / 2"), serde_json::json!(3));
    assert_eq!(eval(&mut scripts, "return 8 / 2.0"), serde_json::json!(4.0));
}

#[test]
fn the_division_option_cannot_change_once_chunks_are_loaded() {
    let mut scripts = scripts();
    scripts.run("game", "d.lua", b"Q = 1").unwrap();
    let error = scripts
        .set_division(DivisionSemantics::IntegerTruncating)
        .unwrap_err()
        .to_string();
    assert!(error.contains("before"), "{error}");
}

#[test]
fn scripts_cannot_reach_natives_through_the_division_function() {
    let mut scripts = integer_dividing();
    let value = eval(&mut scripts, "return type(__lr_div)");
    assert_eq!(value, serde_json::json!("function"));
    let natives = eval(&mut scripts, "return load('return Sleep')() == nil");
    assert_eq!(natives, serde_json::json!(true));
}

/// A thread that divides after it wakes, with operands it holds across the sleep.
const SLEEPY: &str = r#"
Log = {}
function Main()
    local a, b = 7, 2
    Sleep(10)
    Log[#Log + 1] = tostring(a / b)
    Sleep(10)
    Log[#Log + 1] = tostring(load("return 9 / 2")())
end
"#;

fn sleepy(division: DivisionSemantics) -> GameScripts {
    let mut scripts = with_division(division);
    scripts
        .run("game", "sleepy.lua", SLEEPY.as_bytes())
        .unwrap();
    scripts
}

#[test]
fn a_restored_thread_continues_with_the_same_division() {
    let division = DivisionSemantics::IntegerTruncating;
    let mut original = sleepy(division);
    original
        .run("game", "start.lua", b"CreateGameTimeThread(Main)")
        .unwrap();
    original.advance(0).unwrap();
    original.advance(5).unwrap();
    let snapshot: worldrunner_script::LuaSnapshot = serde_json::from_str(
        &serde_json::to_string(&original.save(&["Log"], &["Main"]).unwrap()).unwrap(),
    )
    .unwrap();

    let mut restored = sleepy(division);
    restored.restore(&snapshot, &["Main"]).unwrap();
    restored.advance(5).unwrap();
    restored.advance(10).unwrap();
    assert_eq!(
        eval(&mut restored, "return Log"),
        serde_json::json!(["3", "4"])
    );
}

const REFUSED: &[u8] = b"local __lrs = 1\nSplit = 7 / 2";

#[test]
fn a_refused_chunk_fails_by_default() {
    let mut scripts = scripts();
    let error = scripts
        .run("game", "plain.lua", REFUSED)
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot be made resumable"), "{error}");
    assert!(scripts.plain_chunks().is_empty());
}

#[test]
fn a_refused_chunk_loads_plain_when_allowed_and_is_reported() {
    let mut scripts = integer_dividing();
    scripts.set_refused_chunks(RefusedChunks::LoadPlain);
    scripts.run("game", "plain.lua", REFUSED).unwrap();
    assert_eq!(eval(&mut scripts, "return Split"), serde_json::json!(3));
    let plain = scripts.plain_chunks();
    assert_eq!(plain.keys().collect::<Vec<_>>(), ["plain.lua"]);
    assert!(!plain["plain.lua"].is_empty());

    let loaded = eval(
        &mut scripts,
        "return load('local __lrs = 1 return 9 / 2', '=viaload')()",
    );
    assert_eq!(loaded, serde_json::json!(4));
    assert!(scripts.plain_chunks().contains_key("=viaload"));
}

#[test]
fn a_chunk_that_does_not_parse_is_never_loaded_plain() {
    let mut scripts = integer_dividing();
    scripts.set_refused_chunks(RefusedChunks::LoadPlain);
    assert!(scripts.run("game", "syntax.lua", b"x = = 1").is_err());
    assert!(scripts.plain_chunks().is_empty());
}

#[test]
fn two_texts_under_one_chunk_name_are_an_error() {
    let mut scripts = scripts();
    scripts.run("game", "a.lua", b"X = 1").unwrap();
    scripts.run("game", "a.lua", b"X = 1").unwrap();
    let error = scripts
        .run("game", "a.lua", b"X = 2")
        .unwrap_err()
        .to_string();
    assert!(error.contains("a.lua"), "{error}");
    assert!(error.contains("another chunk"), "{error}");
    assert_eq!(eval(&mut scripts, "return X"), serde_json::json!(1));

    let loaded = eval(
        &mut scripts,
        "local one = load('return 1', '=n') local two, message = load('return 2', '=n')
         return { one ~= nil, two == nil, message }",
    );
    assert_eq!(loaded[0], serde_json::json!(true));
    assert_eq!(loaded[1], serde_json::json!(true));
    assert!(loaded[2].as_str().unwrap().contains("=n"), "{loaded}");
}

#[test]
fn a_collision_can_load_plain_and_leave_the_first_chunk_resumable() {
    let mut scripts = scripts();
    scripts.set_name_collisions(NameCollisions::LoadPlain);
    scripts.run("game", "a.lua", b"X = 1").unwrap();
    scripts.run("game", "a.lua", b"X = 2").unwrap();
    assert_eq!(eval(&mut scripts, "return X"), serde_json::json!(2));
    let plain = scripts.plain_chunks();
    assert!(plain["a.lua"].contains("another chunk"), "{plain:?}");
}

#[test]
fn the_loaded_chunks_are_available_to_the_host_by_their_saved_names() {
    let mut scripts = scripts();
    scripts.run("game", "=Lua/a.lua", b"X = 1").unwrap();
    scripts.run("game", "other", b"Y = 1").unwrap();
    let sources = scripts.import_sources();
    let restore = sources.restore_sources();
    assert!(restore.get("=Lua/a.lua").is_some());
    assert!(restore.get("other").is_some());
    assert!(restore.get("=missing").is_none());
}
