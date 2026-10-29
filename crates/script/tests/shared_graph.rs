//! Scheduler threads and saved data share one graph, and threads made from anonymous functions
//! resume (#147).

use worldrunner_script::{GameScripts, LuaSnapshot, SchedulerNames, ScriptConfig};

const NAMES: [&str; 6] = [
    "CreateGameTimeThread",
    "Sleep",
    "Wakeup",
    "DeleteThread",
    "CurrentThread",
    "GameTime",
];

const PRELUDE: &str = r#"
Log = {}
function Note(s) Log[#Log + 1] = s .. "@" .. GameTime() end
Shared = { count = 0 }
"#;

fn game(code: &str) -> GameScripts {
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
    scripts
        .run("game", "game.lua", format!("{PRELUDE}{code}").as_bytes())
        .unwrap();
    scripts
}

fn through_json(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

fn log(scripts: &mut GameScripts) -> serde_json::Value {
    scripts.eval_json("game", "return Log").unwrap()
}

/// The log after `total` milliseconds when nothing is saved.
fn uninterrupted(code: &str, total: u64) -> serde_json::Value {
    let mut scripts = game(code);
    scripts.run("game", "go.lua", b"Start()").unwrap();
    for _ in 0..total {
        assert_eq!(scripts.advance(1).unwrap(), []);
    }
    log(&mut scripts)
}

/// The log after `total` milliseconds when, before every millisecond, the state is saved and
/// restored into a new `GameScripts` that loaded `code` but has not called `Start`.
fn interrupted(code: &str, total: u64) -> serde_json::Value {
    let mut scripts = game(code);
    scripts.run("game", "go.lua", b"Start()").unwrap();
    for _ in 0..total {
        let snapshot = through_json(&scripts.save(&["Log", "Shared"], &[]).unwrap());
        scripts = game(code);
        scripts.restore(&snapshot, &[]).unwrap();
        assert_eq!(scripts.advance(1).unwrap(), []);
    }
    log(&mut scripts)
}

fn same_when_saved_every_millisecond(code: &str, total: u64) -> serde_json::Value {
    let expected = uninterrupted(code, total);
    assert_eq!(interrupted(code, total), expected);
    expected
}

#[test]
fn threads_sharing_a_table_and_an_upvalue_keep_sharing_after_restores() {
    let log = same_when_saved_every_millisecond(
        r#"
        function Start()
        local hits = 0
        local function bump(who)
            hits = hits + 1
            Shared.count = Shared.count + 1
            Note(who .. " hits " .. hits .. " count " .. Shared.count)
        end
        CreateGameTimeThread(function() for _ = 1, 3 do Sleep(10) bump("a") end end)
        CreateGameTimeThread(function() for _ = 1, 3 do Sleep(15) bump("b") end end)
        end
        "#,
        50,
    );
    assert_eq!(
        log,
        serde_json::json!([
            "a hits 1 count 1@10",
            "b hits 2 count 2@15",
            "a hits 3 count 3@20",
            "b hits 4 count 4@30",
            "a hits 5 count 5@30",
            "b hits 6 count 6@45"
        ])
    );
}

#[test]
fn threads_that_have_not_started_save_their_closure_and_arguments() {
    let code = r#"
        function Start()
            local label = "late"
            Handle = CreateGameTimeThread(function(a, b)
                Sleep(a)
                Shared.count = Shared.count + b
                Note(label .. " " .. Shared.count)
            end, 7, 5)
            Shared.worker = Handle
        end
    "#;
    let mut saved = game(code);
    saved.run("game", "go.lua", b"Start()").unwrap();
    let snapshot = through_json(&saved.save(&["Log", "Shared", "Handle"], &[]).unwrap());
    assert_eq!(snapshot.threads.len(), 1);

    let mut restored = game(code);
    restored.restore(&snapshot, &[]).unwrap();
    let same = restored
        .eval_json("game", "return Shared.worker == Handle")
        .unwrap();
    assert_eq!(same, serde_json::json!(true));
    restored.advance(6).unwrap();
    assert_eq!(log(&mut restored), serde_json::json!({}));
    restored.advance(1).unwrap();
    assert_eq!(log(&mut restored), serde_json::json!(["late 5@7"]));
}

#[test]
fn numeric_for_loops_with_sleeps_resume_with_their_counters() {
    let log = same_when_saved_every_millisecond(
        r#"
        function Start()
        CreateGameTimeThread(function()
            for i = 10, 1, -3 do
                Sleep(4)
                Note("down " .. i)
            end
        end)
        end
        "#,
        20,
    );
    assert_eq!(
        log,
        serde_json::json!(["down 10@4", "down 7@8", "down 4@12", "down 1@16"])
    );
}

#[test]
fn a_thread_holding_another_coroutine_resumes_it_where_it_stopped() {
    let log = same_when_saved_every_millisecond(
        r#"
        function Start()
        CreateGameTimeThread(function()
            local co = coroutine.create(function()
                for i = 1, 3 do coroutine.yield(i * 10) end
                return "done"
            end)
            local _, first = coroutine.resume(co)
            Note("co " .. tostring(first))
            while coroutine.status(co) ~= "dead" do
                Sleep(5)
                local _, value = coroutine.resume(co)
                Note("co " .. tostring(value))
            end
        end)
        end
        "#,
        20,
    );
    assert_eq!(
        log,
        serde_json::json!(["co 10@0", "co 20@5", "co 30@10", "co done@15"])
    );
}
