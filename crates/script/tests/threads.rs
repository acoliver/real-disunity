//! Threads and timers for original scripts, bound under the game's own names (#147).

use worldrunner_script::{GameScripts, SchedulerNames, ScriptConfig};

/// A game that names its scheduler as Haemimont's engines do and grants it to its scripts.
fn game() -> GameScripts {
    let mut scripts = GameScripts::new(ScriptConfig::default()).unwrap();
    scripts
        .register_scheduler(&SchedulerNames {
            spawn: "CreateGameTimeThread".into(),
            sleep: "Sleep".into(),
            wakeup: "Wakeup".into(),
            kill: "DeleteThread".into(),
            current: "CurrentThread".into(),
            now: "GameTime".into(),
        })
        .unwrap();
    let all = [
        "CreateGameTimeThread",
        "Sleep",
        "Wakeup",
        "DeleteThread",
        "CurrentThread",
        "GameTime",
    ];
    scripts.add_source("game", &all).unwrap();
    scripts.add_source("mod", &["Sleep"]).unwrap();
    scripts
        .run(
            "game",
            "log.lua",
            b"Log = {} function Note(s) Log[#Log + 1] = s .. '@' .. GameTime() end",
        )
        .unwrap();
    scripts
}

fn log(scripts: &mut GameScripts) -> serde_json::Value {
    scripts.eval_json("game", "return Log").unwrap()
}

#[test]
fn threads_wake_in_time_order_within_an_advance() {
    let mut scripts = game();
    scripts
        .run(
            "game",
            "units.lua",
            br#"
            CreateGameTimeThread(function(name, period)
                for _ = 1, 3 do Sleep(period) Note(name) end
            end, "fast", 30)
            CreateGameTimeThread(function() Sleep(50) Note("slow") end)
            CreateGameTimeThread(function() Note("first") end)
            "#,
        )
        .unwrap();
    assert_eq!(
        scripts.eval_json("game", "return #Log").unwrap(),
        serde_json::json!(0),
        "threads start at the next advance"
    );
    assert!(scripts.advance(100).unwrap().is_empty());
    assert_eq!(
        log(&mut scripts),
        serde_json::json!(["first@0", "fast@30", "slow@50", "fast@60", "fast@90"])
    );
    assert_eq!(scripts.now(), 100);
    assert_eq!(scripts.thread_count(), 0, "finished threads are dropped");
}

#[test]
fn wakeup_kill_and_waiting_until_woken() {
    let mut scripts = game();
    scripts
        .run(
            "game",
            "waits.lua",
            br#"
            Waiter = CreateGameTimeThread(function() Sleep(math.huge) Note("woken") end)
            Doomed = CreateGameTimeThread(function() Sleep(10) Note("never") end)
            Quitter = CreateGameTimeThread(function()
                Note("quitting") DeleteThread(CurrentThread()) Sleep(1) Note("after quit")
            end)
            "#,
        )
        .unwrap();
    scripts.advance(5).unwrap();
    assert_eq!(scripts.thread_count(), 2);
    let killed = scripts
        .eval_json(
            "game",
            "return { DeleteThread(Doomed), DeleteThread(Doomed), Wakeup(Waiter), Wakeup(Doomed) }",
        )
        .unwrap();
    assert_eq!(killed, serde_json::json!([true, false, true, false]));
    scripts.advance(1_000).unwrap();
    assert_eq!(
        log(&mut scripts),
        serde_json::json!(["quitting@0", "woken@5"])
    );
    assert_eq!(scripts.thread_count(), 0);
}

#[test]
fn zero_sleeps_wait_for_the_next_advance_and_errors_end_one_thread() {
    let mut scripts = game();
    scripts
        .run(
            "game",
            "loops.lua",
            br#"
            Spins = 0
            CreateGameTimeThread(function() while true do Spins = Spins + 1 Sleep(0) end end)
            CreateGameTimeThread(function() Sleep(1) error("broken unit") end)
            CreateGameTimeThread(function() Sleep(2) Note("survivor") end)
            "#,
        )
        .unwrap();
    let failures = scripts.advance(10).unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message.contains("broken unit"), "{failures:?}");
    assert_eq!(
        scripts.eval_json("game", "return Spins").unwrap(),
        serde_json::json!(1)
    );
    scripts.advance(10).unwrap();
    scripts.advance(10).unwrap();
    assert_eq!(
        scripts.eval_json("game", "return Spins").unwrap(),
        serde_json::json!(3)
    );
    assert_eq!(log(&mut scripts), serde_json::json!(["survivor@2"]));
}

#[test]
fn each_resume_has_its_own_budget_and_bad_sleeps_are_refused() {
    let config = ScriptConfig {
        instruction_budget: 100_000,
        ..ScriptConfig::default()
    };
    let mut scripts = GameScripts::new(config).unwrap();
    scripts
        .register_scheduler(&SchedulerNames {
            spawn: "Go".into(),
            sleep: "Sleep".into(),
            wakeup: "Wakeup".into(),
            kill: "Kill".into(),
            current: "Current".into(),
            now: "Now".into(),
        })
        .unwrap();
    scripts.add_source("game", &["Go", "Sleep"]).unwrap();
    scripts
        .run(
            "game",
            "work.lua",
            br#"
            Done = 0
            Go(function() for _ = 1, 10 do for _ = 1, 5000 do end Sleep(1) Done = Done + 1 end end)
            Go(function() while true do end end)
            Go(function() Sleep(-1) end)
            Go(function() Sleep("soon") end)
            "#,
        )
        .unwrap();
    let failures = scripts.advance(100).unwrap();
    let messages: Vec<&str> = failures.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(failures.len(), 3, "{messages:?}");
    assert!(messages[0].contains("instruction budget"), "{messages:?}");
    assert!(
        messages[1..]
            .iter()
            .all(|m| m.contains("non-negative number")),
        "{messages:?}"
    );
    assert_eq!(
        scripts.eval_json("game", "return Done").unwrap(),
        serde_json::json!(10),
        "a thread whose whole run exceeds one budget completes across resumes"
    );
    let outside = scripts.run("game", "outside.lua", b"Sleep(5)").unwrap_err();
    assert!(outside.to_string().contains("yield"), "{outside}");
}

#[test]
fn scheduler_names_are_natives_with_grants() {
    let mut scripts = game();
    let seen = scripts
        .eval_json(
            "mod",
            "return { Sleep ~= nil, CreateGameTimeThread == nil, GameTime == nil }",
        )
        .unwrap();
    assert_eq!(seen, serde_json::json!([true, true, true]));
    let replaced = scripts
        .eval_json(
            "game",
            "coroutine.yield = function() error('replaced') end CreateGameTimeThread(function() Sleep(5) Note('kept') end) return true",
        )
        .unwrap();
    assert_eq!(replaced, serde_json::json!(true));
    assert!(
        scripts.advance(10).unwrap().is_empty(),
        "Sleep uses the yield captured at registration"
    );
    assert_eq!(log(&mut scripts), serde_json::json!(["kept@5"]));
}
