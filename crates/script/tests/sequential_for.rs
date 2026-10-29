//! A scheduler thread with several numeric `for` loops in one function restores from any point
//! (#147): the loops after the restored one keep their own limits, steps and the locals declared
//! between them.

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
"#;

/// An integer loop followed by a loop with a float step.
const TWO_LOOPS: &str = r#"
function Start()
    CreateGameTimeThread(function()
        local total = 0
        for i = 1, 3 do
            Sleep(5)
            total = total + i
            Note(string.format("a%d %g", i, total))
        end
        for x = 1, 2, 0.5 do
            Sleep(7)
            total = total + x
            Note(string.format("b%.1f %g", x, total))
        end
    end)
end
"#;

/// Integer, float and descending integer loops, with locals declared between them that the
/// last loop reads.
const THREE_LOOPS: &str = r#"
function Start()
    CreateGameTimeThread(function()
        local total = 0
        for i = 1, 3 do
            Sleep(4)
            total = total + i
            Note(string.format("a%d %g", i, total))
        end
        local mid = total * 10
        for x = 0.5, 1.5, 0.5 do
            Sleep(6)
            total = total + x
            Note(string.format("b%.1f %g", x, total))
        end
        local late = { mid, total }
        for k = 9, 3, -3 do
            Sleep(5)
            Note(string.format("c%d %g", k, mid + late[2] + k))
        end
        Note(string.format("end %g %g", mid, late[2]))
    end)
end
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

fn started(code: &str) -> GameScripts {
    let mut scripts = game(code);
    scripts.run("game", "go.lua", b"Start()").unwrap();
    scripts
}

fn through_json(snapshot: &LuaSnapshot) -> LuaSnapshot {
    serde_json::from_str(&serde_json::to_string(snapshot).unwrap()).unwrap()
}

/// Saves `scripts` and restores the snapshot into a new `GameScripts` that has loaded `code`
/// but has not called `Start`.
fn moved_to_fresh(scripts: &GameScripts, code: &str) -> GameScripts {
    let snapshot = through_json(&scripts.save(&["Log"], &[]).unwrap());
    let mut fresh = game(code);
    fresh.restore(&snapshot, &[]).unwrap();
    fresh
}

fn log(scripts: &mut GameScripts) -> serde_json::Value {
    scripts.eval_json("game", "return Log").unwrap()
}

fn uninterrupted(code: &str, total: u64) -> serde_json::Value {
    let mut scripts = started(code);
    assert_eq!(scripts.advance(total).unwrap(), []);
    log(&mut scripts)
}

/// Saves at every millisecond, restores into a fresh state and runs the rest of the way there.
fn finished_after_one_restore(code: &str, total: u64) {
    let expected = uninterrupted(code, total);
    for saved_at in 0..=total {
        let scripts = started(code);
        assert_eq!(scripts.advance(saved_at).unwrap(), []);
        let mut restored = moved_to_fresh(&scripts, code);
        assert_eq!(restored.advance(total - saved_at).unwrap(), []);
        assert_eq!(log(&mut restored), expected, "saved at {saved_at}");
    }
}

/// Restores, runs to the next wake, saves and restores again until `total` milliseconds passed.
/// A snapshot with no live threads does not carry the clock, so the elapsed time is counted here.
fn finished_after_a_restore_at_every_wake(code: &str, total: u64) {
    let expected = uninterrupted(code, total);
    let mut scripts = started(code);
    let mut elapsed = 0;
    let mut restores = 0;
    while elapsed < total {
        let logged = log_len(&mut scripts);
        while elapsed < total && log_len(&mut scripts) == logged {
            assert_eq!(scripts.advance(1).unwrap(), []);
            elapsed += 1;
        }
        scripts = moved_to_fresh(&scripts, code);
        restores += 1;
    }
    assert_eq!(log(&mut scripts), expected);
    assert!(restores >= 4, "only {restores} restores");
}

fn log_len(scripts: &mut GameScripts) -> u64 {
    scripts
        .eval_json("game", "return #Log")
        .unwrap()
        .as_u64()
        .unwrap()
}

#[test]
fn an_integer_loop_then_a_float_step_loop_runs_the_same_after_restores() {
    assert_eq!(
        uninterrupted(TWO_LOOPS, 40),
        serde_json::json!([
            "a1 1@5",
            "a2 3@10",
            "a3 6@15",
            "b1.0 7@22",
            "b1.5 8.5@29",
            "b2.0 10.5@36"
        ])
    );
    finished_after_one_restore(TWO_LOOPS, 40);
    finished_after_a_restore_at_every_wake(TWO_LOOPS, 40);
}

#[test]
fn three_loops_with_locals_between_them_run_the_same_after_restores() {
    assert_eq!(
        uninterrupted(THREE_LOOPS, 50),
        serde_json::json!([
            "a1 1@4",
            "a2 3@8",
            "a3 6@12",
            "b0.5 6.5@18",
            "b1.0 7.5@24",
            "b1.5 9@30",
            "c9 78@35",
            "c6 75@40",
            "c3 72@45",
            "end 60 9@45"
        ])
    );
    finished_after_one_restore(THREE_LOOPS, 50);
    finished_after_a_restore_at_every_wake(THREE_LOOPS, 50);
}
