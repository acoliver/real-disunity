//! Saving a game's Lua data and restoring it into a fresh state that runs the current code
//! (#147).

use worldrunner_script::{GameScripts, LuaSnapshot, Saved, ScriptConfig, ScriptError};

/// The game's code: a class with a method, version `bonus` of its damage rule.
fn code(bonus: i64) -> String {
    format!(
        "Unit = {{}} Unit.__index = Unit
         function Unit.Hit(self, n) self.hp = self.hp - n - {bonus} return self.hp end
         function Unit.New(hp) return setmetatable({{ hp = hp }}, Unit) end"
    )
}

fn game(bonus: i64) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_native("GetMapName", |_, ()| Ok("Crater"))
        .unwrap();
    scripts.add_source("game", &["GetMapName"]).unwrap();
    scripts
        .run("game", "unit.lua", code(bonus).as_bytes())
        .unwrap();
    scripts
}

const STATE: &str = r#"
local a = Unit.New(10)
State = {
    units = { a, a, Unit.New(4) },
    by_unit = { [a] = "leader" },
    counts = { n = 3, f = 3.0, nan = 0/0, big = math.maxinteger },
    bytes = "\0\255",
    map = GetMapName,
    hit = Unit.Hit,
}
State.self = State
Score = 7
"#;

fn round_trip(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

#[test]
fn shared_tables_cycles_and_number_types_survive_a_round_trip() {
    let mut saved = game(0);
    saved.run("game", "state.lua", STATE.as_bytes()).unwrap();
    let snapshot = round_trip(&saved.save(&["State", "Score"], &["Unit"]).unwrap());

    let mut restored = game(0);
    restored.restore(&snapshot, &["Unit"]).unwrap();
    let checks = restored
        .eval_json(
            "game",
            r#"local s = State
            return {
                s.units[1] == s.units[2], s.units[1] ~= s.units[3], s.self == s,
                s.by_unit[s.units[1]], math.type(s.counts.n), math.type(s.counts.f),
                s.counts.nan ~= s.counts.nan, s.counts.big == math.maxinteger,
                s.bytes == "\0\255", s.map(), Score,
            }"#,
        )
        .unwrap();
    assert_eq!(
        checks,
        serde_json::json!([
            true, true, true, "leader", "integer", "float", true, true, true, "Crater", 7
        ])
    );
}

#[test]
fn permanents_rebind_to_the_current_code_by_name() {
    let mut saved = game(0);
    saved.run("game", "state.lua", STATE.as_bytes()).unwrap();
    let snapshot = saved.save(&["State"], &["Unit"]).unwrap();
    let names: Vec<&Saved> = snapshot
        .tables
        .iter()
        .flat_map(|table| {
            table
                .entries
                .iter()
                .map(|(_, value)| value)
                .chain(&table.metatable)
        })
        .filter(|value| matches!(value, Saved::Native(_) | Saved::Permanent(_)))
        .collect();
    assert!(
        names.contains(&&Saved::Permanent("Unit".into())),
        "{names:?}"
    );
    assert!(
        names.contains(&&Saved::Permanent("Unit.Hit".into())),
        "{names:?}"
    );
    assert!(
        names.contains(&&Saved::Native("GetMapName".into())),
        "{names:?}"
    );

    // The game was updated: Hit now deals one more damage. Restored units use the new rule.
    let mut updated = game(1);
    updated.restore(&snapshot, &["Unit"]).unwrap();
    let hp = updated
        .eval_json(
            "game",
            "return { State.units[1]:Hit(2), State.hit(State.units[3], 1) }",
        )
        .unwrap();
    assert_eq!(hp, serde_json::json!([7, 2]));
}

#[test]
fn snapshots_do_not_depend_on_insertion_order() {
    let mut first = game(0);
    first
        .run(
            "game",
            "a.lua",
            b"T = {} T.b = 2 T.a = 1 T[2] = 'y' T[1] = 'x' T.c = { z = 1, y = 2 }",
        )
        .unwrap();
    let mut second = game(0);
    second
        .run(
            "game",
            "b.lua",
            b"T = {} T[1] = 'x' T.c = { y = 2, z = 1 } T.a = 1 T[2] = 'y' T.b = 2",
        )
        .unwrap();
    assert_eq!(
        first.save(&["T"], &["Unit"]).unwrap(),
        second.save(&["T"], &["Unit"]).unwrap()
    );
}

fn refusal(result: Result<impl std::fmt::Debug, ScriptError>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn values_that_cannot_be_saved_are_refused_with_their_location() {
    let mut scripts = game(0);
    scripts
        .run(
            "game",
            "bad.lua",
            b"Closure = { inner = { on_death = function() end } }
              Thread = { co = coroutine.create(function() end) }",
        )
        .unwrap();
    let closure = refusal(scripts.save(&["Closure"], &["Unit"]));
    assert!(
        closure.contains(r#"Closure["inner"]["on_death"] is a function that is not a permanent"#),
        "{closure}"
    );
    let thread = refusal(scripts.save(&["Thread"], &["Unit"]));
    assert!(
        thread.contains(r#"Thread["co"] is a suspended coroutine the scheduler does not own"#),
        "{thread}"
    );
    let root = refusal(scripts.save(&["Unit"], &["Unit"]));
    assert!(root.contains("root Unit is a permanent"), "{root}");
    let scalar = refusal(scripts.save(&["Closure"], &["Score"]));
    assert!(scalar.contains("permanent root Score is a nil"), "{scalar}");
}

#[test]
fn snapshots_that_do_not_fit_the_state_are_refused() {
    let mut saved = game(0);
    saved.run("game", "state.lua", STATE.as_bytes()).unwrap();
    let snapshot = saved.save(&["State"], &["Unit"]).unwrap();

    let mut renamed = GameScripts::new(ScriptConfig::default()).unwrap();
    renamed
        .register_native("GetMapName", |_, ()| Ok("Crater"))
        .unwrap();
    renamed.add_source("game", &["GetMapName"]).unwrap();
    renamed
        .run(
            "game",
            "unit.lua",
            code(0).replace("Unit", "Soldier").as_bytes(),
        )
        .unwrap();
    let missing = refusal(renamed.restore(&snapshot, &["Soldier"]));
    assert!(missing.contains("permanent Unit"), "{missing}");

    let mut broken = snapshot.clone();
    broken.tables[0]
        .entries
        .push((Saved::String(b"x".to_vec()), Saved::Table(999)));
    let index = refusal(game(0).restore(&broken, &["Unit"]));
    assert!(
        index.contains("table 999 is not in the snapshot"),
        "{index}"
    );

    let mut nested = snapshot;
    nested.roots[0].0 = "Missing.State".into();
    let parent = refusal(game(0).restore(&nested, &["Unit"]));
    assert!(parent.contains("its parent is not a table"), "{parent}");
}
