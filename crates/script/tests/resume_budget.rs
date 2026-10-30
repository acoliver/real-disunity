//! The per-resume budget of scheduler threads: overruns that `pcall` cannot swallow, the wall
//! time limit, and the budget of coroutines a thread creates (#147).

use std::time::{Duration, Instant};

use worldrunner_script::{
    GameScripts, OnOverrun, ResumeBudget, SavedWake, SchedulerNames, ScriptConfig, ScriptError,
};

const NAMES: [&str; 7] = [
    "Go", "Sleep", "Wakeup", "Kill", "Current", "Now", "SlowFail",
];

fn names() -> SchedulerNames {
    SchedulerNames {
        spawn: NAMES[0].into(),
        sleep: NAMES[1].into(),
        wakeup: NAMES[2].into(),
        kill: NAMES[3].into(),
        current: NAMES[4].into(),
        now: NAMES[5].into(),
    }
}

fn budget(instructions: u64, on_overrun: OnOverrun) -> ResumeBudget {
    ResumeBudget {
        instructions,
        check_every: 1_000,
        wall_time: Duration::from_secs(60),
        on_overrun,
    }
}

fn game(config: ScriptConfig) -> GameScripts {
    let mut scripts = GameScripts::new(config).unwrap();
    // A native that takes a while and then fails: a few Lua instructions per millisecond.
    scripts
        .register_native("SlowFail", |_, ()| -> mlua::Result<()> {
            std::thread::sleep(Duration::from_millis(5));
            Err(mlua::Error::runtime("native failure"))
        })
        .unwrap();
    scripts.register_scheduler(&names()).unwrap();
    scripts.add_source("game", &NAMES).unwrap();
    scripts
}

fn with_resume(resume: ResumeBudget) -> GameScripts {
    game(ScriptConfig {
        resume_budget: Some(resume),
        ..ScriptConfig::default()
    })
}

fn messages(scripts: &GameScripts, ms: u64) -> Vec<String> {
    let failures = scripts.advance(ms).unwrap();
    failures
        .into_iter()
        .map(|failure| failure.message)
        .collect()
}

const PCALL_LOOP: &[u8] = b"Survivors = 0
Go(function()
    while true do pcall(function() end) end
end)
Go(function() Survivors = Survivors + 1 end)
";

#[test]
fn a_pcall_loop_ends_with_a_located_report() {
    let mut scripts = with_resume(budget(200_000, OnOverrun::EndThreadWithReport));
    scripts.run("game", "spin.lua", PCALL_LOOP).unwrap();
    let failures = messages(&scripts, 10);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].starts_with("budget exceeded at "),
        "{failures:?}"
    );
    let first_line = failures[0].lines().next().unwrap();
    assert!(first_line.contains("spin.lua\"]:3"), "{first_line}");
    assert!(failures[0].contains("stack traceback:"), "{failures:?}");
    assert_eq!(scripts.thread_count(), 0, "the overrunning thread ended");
    assert_eq!(
        scripts.eval_json("game", "return Survivors").unwrap(),
        serde_json::json!(1),
        "the other thread of the advance still ran"
    );
}

#[test]
fn raise_error_reports_the_overrun_as_an_error() {
    let mut scripts = with_resume(budget(200_000, OnOverrun::RaiseError));
    scripts.run("game", "spin.lua", PCALL_LOOP).unwrap();
    let failures = messages(&scripts, 10);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].contains("instruction budget of 200000 exceeded"),
        "{failures:?}"
    );
    assert_eq!(scripts.thread_count(), 0);
}

#[test]
fn a_native_error_loop_is_stopped_by_the_wall_time() {
    for on_overrun in [OnOverrun::EndThreadWithReport, OnOverrun::RaiseError] {
        let mut scripts = with_resume(ResumeBudget {
            instructions: 1_000_000_000_000,
            check_every: 10,
            wall_time: Duration::from_millis(150),
            on_overrun,
        });
        scripts
            .run(
                "game",
                "slow.lua",
                b"Go(function() while true do pcall(SlowFail) end end)",
            )
            .unwrap();
        let started = Instant::now();
        let failures = messages(&scripts, 10);
        assert!(started.elapsed() >= Duration::from_millis(150));
        assert!(started.elapsed() < Duration::from_secs(20));
        assert_eq!(failures.len(), 1, "{on_overrun:?}: {failures:?}");
        match on_overrun {
            OnOverrun::EndThreadWithReport => {
                assert!(failures[0].contains("slow.lua\"]:1"), "{failures:?}");
            }
            OnOverrun::RaiseError => assert!(failures[0].contains("wall time"), "{failures:?}"),
        }
    }
}

#[test]
fn a_thread_within_the_budget_is_unaffected() {
    let mut scripts = with_resume(ResumeBudget {
        check_every: 1_000,
        ..ResumeBudget::default()
    });
    scripts
        .run(
            "game",
            "steady.lua",
            b"Done = 0
              Go(function()
                  for round = 1, 3 do
                      local sum = 0
                      for i = 1, 350000 do sum = sum + i end
                      Done = Done + 1
                      Sleep(10)
                  end
              end)",
        )
        .unwrap();
    assert_eq!(messages(&scripts, 100), Vec::<String>::new());
    assert_eq!(
        scripts.eval_json("game", "return Done").unwrap(),
        serde_json::json!(3)
    );
}

#[test]
fn a_coroutine_a_thread_creates_runs_under_the_resume_budget() {
    let mut scripts = with_resume(budget(200_000, OnOverrun::EndThreadWithReport));
    scripts
        .run(
            "game",
            "nested.lua",
            b"Go(function()
                  local inner = coroutine.wrap(function()
                      while true do pcall(function() end) end
                  end)
                  inner()
                  Reached = true
              end)",
        )
        .unwrap();
    let failures = messages(&scripts, 10);
    assert_eq!(failures.len(), 1, "{failures:?}");
    let first_line = failures[0].lines().next().unwrap();
    assert!(first_line.contains("nested.lua\"]:3"), "{first_line}");
    assert_eq!(scripts.thread_count(), 0);
}

#[test]
fn loading_keeps_its_own_budget_and_the_next_resume_starts_fresh() {
    let mut scripts = game(ScriptConfig {
        instruction_budget: 5_000_000,
        resume_budget: Some(budget(100_000, OnOverrun::EndThreadWithReport)),
        ..ScriptConfig::default()
    });
    scripts
        .run(
            "game",
            "boot.lua",
            b"local n = 0 for i = 1, 400000 do n = n + i end Booted = n",
        )
        .unwrap();
    scripts
        .run(
            "game",
            "threads.lua",
            b"Go(function() for i = 1, 400000 do end end)
              Go(function() Sleep(5) Late = true end)",
        )
        .unwrap();
    assert_eq!(messages(&scripts, 10).len(), 1, "the first thread overran");
    assert_eq!(
        scripts.eval_json("game", "return Late").unwrap(),
        serde_json::json!(true),
        "an overrun leaves the next resume with a fresh budget"
    );
    let entry = scripts
        .run("game", "spin.lua", b"while true do end")
        .unwrap_err();
    assert!(
        matches!(
            entry,
            ScriptError::BudgetExceeded {
                budget: 5_000_000,
                ..
            }
        ),
        "{entry:?}"
    );
}

#[test]
fn threads_resumed_under_the_budget_still_save_and_restore() {
    const DEFS: &[u8] = b"Ticks = {}
        function Walk(name)
            while true do
                Sleep(30)
                Ticks[#Ticks + 1] = name .. Now()
            end
        end";
    let config = || ScriptConfig {
        resume_budget: Some(ResumeBudget::default()),
        ..ScriptConfig::default()
    };
    let mut saved = game(config());
    saved.run("game", "defs.lua", DEFS).unwrap();
    saved
        .run("game", "start.lua", b"Unit = { thread = Go(Walk, 'u') }")
        .unwrap();
    assert_eq!(messages(&saved, 70), Vec::<String>::new());
    let snapshot = saved.save(&["Unit", "Ticks"], &["Walk"]).unwrap();
    assert_eq!(snapshot.threads.len(), 1);
    assert_eq!(snapshot.threads[0].wake, SavedWake::At(90));

    let mut restored = game(config());
    restored.run("game", "defs.lua", DEFS).unwrap();
    restored.restore(&snapshot, &["Walk"]).unwrap();
    assert_eq!(messages(&restored, 50), Vec::<String>::new());
    assert_eq!(
        restored.eval_json("game", "return Ticks").unwrap(),
        serde_json::json!(["u30", "u60", "u90", "u120"])
    );
}
