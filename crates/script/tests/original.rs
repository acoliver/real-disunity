//! A game's original scripts call the natives the game registers, and each source of scripts
//! reaches only the natives the game grants it (#147).

use std::sync::{Arc, Mutex};

use worldrunner_script::{
    Edit, GameScripts, Overlay, Patch, PatchSet, ScriptConfig, ScriptError, sha256_hex,
};

/// A game with three natives: a harmless query, a namespaced one, and one that writes files.
fn game(saved: &Arc<Mutex<Vec<String>>>) -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_native("GetMapName", |_, ()| Ok("Crater"))
        .unwrap();
    scripts
        .register_native("terrain.GetHeight", |_, (x, y): (i64, i64)| Ok(x + y))
        .unwrap();
    let sink = Arc::clone(saved);
    scripts
        .register_native("SaveFile", move |_, path: String| {
            sink.lock().unwrap().push(path);
            Ok(())
        })
        .unwrap();
    scripts
        .add_source("game", &["GetMapName", "terrain.GetHeight", "SaveFile"])
        .unwrap();
    scripts.add_source("mod", &["GetMapName"]).unwrap();
    scripts
}

const GAME: &str = r#"
terrain = { Scale = 3 }
function MapTitle() return GetMapName() .. " at " .. terrain.GetHeight(1, terrain.Scale) end
function Persist(path) SaveFile(path) end
"#;

#[test]
fn each_source_reaches_only_its_granted_natives() {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut scripts = game(&saved);
    scripts.run("game", "game.lua", GAME.as_bytes()).unwrap();
    let title: String = scripts.call("MapTitle", ()).unwrap();
    assert_eq!(
        title, "Crater at 4",
        "a native table still reads the game's own terrain table"
    );

    let seen = scripts
        .eval_json(
            "mod",
            "ModLoaded = true return { GetMapName(), SaveFile == nil, terrain.GetHeight == nil, terrain.Scale }",
        )
        .unwrap();
    assert_eq!(seen, serde_json::json!(["Crater", true, true, 3]));
    assert_eq!(
        scripts.eval_json("game", "return ModLoaded").unwrap(),
        serde_json::json!(true),
        "sources share the game's global namespace"
    );

    let denied = scripts
        .run("mod", "mod.lua", b"SaveFile('/tmp/owned')")
        .unwrap_err();
    assert!(denied.to_string().contains("SaveFile"), "{denied}");
    // A game function keeps the game's natives when a mod calls it, as in the original engine.
    scripts
        .run("mod", "mod2.lua", b"Persist('slot1.sav')")
        .unwrap();
    assert_eq!(*saved.lock().unwrap(), ["slot1.sav"]);
}

#[test]
fn natives_are_out_of_reach_of_load_and_the_shared_namespace() {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut scripts = game(&saved);
    let seen = scripts
        .eval_json(
            "game",
            "return { load('return SaveFile')() == nil, _G.SaveFile == nil, GetMapName ~= nil }",
        )
        .unwrap();
    assert_eq!(seen, serde_json::json!([true, true, true]));
    // Writes reach the shared namespace, and a native namespace survives the game rebinding
    // or extending it.
    let writes = scripts
        .eval_json(
            "game",
            "terrain = {} terrain.Extra = 1 GetMapName = nil return { terrain.Extra, terrain.GetHeight(1, 1), GetMapName() }",
        )
        .unwrap();
    assert_eq!(writes, serde_json::json!([1, 2, "Crater"]));
    let shared = scripts
        .eval_json(
            "mod",
            "return { terrain.Extra, rawget(_G, 'terrain') ~= nil }",
        )
        .unwrap();
    assert_eq!(shared, serde_json::json!([1, true]));
}

#[test]
fn natives_and_sources_are_checked_when_registered() {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts.register_native("Ok", |_, ()| Ok(1)).unwrap();
    for (path, expected) in [
        ("Ok", "registered twice"),
        ("bad name", "not a dotted Lua name"),
        ("a..b", "not a dotted Lua name"),
        ("9lives", "not a dotted Lua name"),
        ("Ok.Inner", "nests with native Ok"),
    ] {
        let error = scripts.register_native(path, |_, ()| Ok(())).unwrap_err();
        assert!(error.to_string().contains(expected), "{path}: {error}");
    }
    let unknown = scripts.add_source("mod", &["Missing"]).unwrap_err();
    assert!(
        unknown.to_string().contains("unknown native Missing"),
        "{unknown}"
    );
    scripts.add_source("game", &["Ok"]).unwrap();
    assert!(
        scripts.add_source("game", &[]).is_err(),
        "sources are unique"
    );
    let late = scripts.register_native("Late", |_, ()| Ok(())).unwrap_err();
    assert!(late.to_string().contains("after sources"), "{late}");
    let missing = scripts.run("nobody", "x.lua", b"return 1").unwrap_err();
    assert!(
        missing.to_string().contains("unknown source nobody"),
        "{missing}"
    );
    let absent = scripts.call::<()>("NoSuchFunction", ()).unwrap_err();
    assert!(
        absent.to_string().contains("not a function (found nil)"),
        "{absent}"
    );
}

#[test]
fn original_files_load_patched_as_text_under_the_budget() {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut scripts = game(&saved);
    let original =
        "function Sum(...) local n = 0 for _, v in ipairs(arg) do n = n + v end return n end";
    let overlay = Overlay::new([PatchSet {
        file: "sum.lua".into(),
        original_sha256: sha256_hex(original.as_bytes()),
        patches: vec![Patch {
            name: "lua54-varargs".into(),
            edits: vec![Edit {
                find: "local n = 0".into(),
                replace: "local arg = {...} local n = 0".into(),
            }],
        }],
    }])
    .unwrap();
    let applied = scripts
        .run_patched("game", "sum.lua", original.as_bytes(), &overlay)
        .unwrap();
    assert_eq!(applied, ["lua54-varargs"]);
    assert_eq!(scripts.call::<i64>("Sum", (1, 2, 3)).unwrap(), 6);

    let bytecode = mlua::Lua::new()
        .load("return 1")
        .into_function()
        .unwrap()
        .dump(true);
    let binary = scripts.run("game", "compiled.lua", &bytecode).unwrap_err();
    assert!(binary.to_string().contains("binary"), "{binary}");

    scripts
        .run("game", "loop.lua", b"function Spin() while true do end end")
        .unwrap();
    let spun = scripts.call::<()>("Spin", ()).unwrap_err();
    assert!(
        matches!(spun, ScriptError::BudgetExceeded { ref chunk, .. } if chunk == "Spin"),
        "{spun}"
    );
    let threads = scripts
        .eval_json(
            "mod",
            "local co = coroutine.wrap(function() coroutine.yield(7) end) return co()",
        )
        .unwrap();
    assert_eq!(
        threads,
        serde_json::json!(7),
        "the coroutine library is available"
    );
}

#[test]
fn coroutines_run_under_the_budget_and_resume_as_in_lua() {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let mut scripts = game(&saved);
    // Coroutines count against the same budget, however deeply they nest.
    let nested = scripts
        .run(
            "game",
            "nested.lua",
            b"function Nested() coroutine.wrap(function() coroutine.wrap(function() while true do end end)() end)() end",
        )
        .and_then(|()| scripts.call::<()>("Nested", ()))
        .unwrap_err();
    assert!(
        matches!(nested, ScriptError::BudgetExceeded { .. }),
        "{nested}"
    );
    let resumed = scripts
        .eval_json(
            "game",
            "local co = coroutine.create(function(a) local b = coroutine.yield(a + 1) error('stop ' .. b, 0) end) \
             local _, first = coroutine.resume(co, 1) local ok, message = coroutine.resume(co, 'here') \
             return { first, ok, message, coroutine.status(co) }",
        )
        .unwrap();
    assert_eq!(resumed, serde_json::json!([2, false, "stop here", "dead"]));
}
