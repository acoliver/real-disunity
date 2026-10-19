//! Contract tests from issue #206: sandbox, instruction budget, determinism,
//! handle lifetime, and api-version enforcement.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use worldrunner_script::{
    EntityHandle, HookContext, ScriptConfig, ScriptError, ScriptHost, WorldView,
};

/// Flat one-entity world whose generation the test controls.
struct TestWorld {
    generation: AtomicU64,
}

impl WorldView for TestWorld {
    fn terrain_height(&self, east: f64, north: f64) -> Option<f64> {
        Some(east * 0.5 + north * 0.25)
    }

    fn entities_by_family(&self, family: &str) -> Vec<EntityHandle> {
        if family == "trees" {
            vec![EntityHandle {
                id: 7,
                generation: self.generation.load(Ordering::Relaxed),
            }]
        } else {
            Vec::new()
        }
    }

    fn entity_family(&self, handle: EntityHandle) -> Option<String> {
        if handle.generation == self.generation.load(Ordering::Relaxed) && handle.id == 7 {
            Some("trees".into())
        } else {
            None
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

fn host_with(script: &str) -> ScriptHost {
    let mut host = ScriptHost::new(ScriptConfig::default()).expect("host builds");
    host.load_module("test", script).expect("script loads");
    host
}

#[test]
fn os_io_debug_and_ffi_are_absent() {
    let mut host = host_with("return 1");
    for name in ["os", "io", "debug", "ffi", "jit", "package"] {
        let value = host
            .eval_json(&format!("{name} == nil"))
            .expect("evaluates");
        assert_eq!(value, serde_json::Value::Bool(true), "{name} must be nil");
    }
}

#[test]
fn require_of_an_unknown_module_is_an_error_naming_it() {
    let mut host = host_with("return 1");
    let error = host
        .eval_json("require('os')")
        .expect_err("require('os') fails");
    let message = error.to_string();
    assert!(
        message.contains("os"),
        "error must name the module: {message}"
    );
}

#[test]
fn in_memory_require_resolves_and_caches() {
    let mut host = ScriptHost::new(ScriptConfig::default()).expect("host builds");
    host.load_module("util", "return { marker = 41 + 1 }")
        .expect("loads");
    let value = host
        .eval_json("local u = require('util'); return u.marker")
        .expect("evaluates");
    assert_eq!(value, serde_json::json!(42));
}

#[test]
fn budget_exceeded_halts_a_runaway_hook() {
    let config = ScriptConfig {
        instruction_budget: 5_000,
        seed: 0,
    };
    let mut host = ScriptHost::new(config).expect("host builds");
    host.load_module("spin", "function on_tick(dt, frame) while true do end end")
        .expect("loads");
    match host.fire("on_tick", &HookContext::default()) {
        Err(ScriptError::BudgetExceeded { budget, .. }) => assert_eq!(budget, 5_000),
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}

#[test]
fn same_seed_same_trace_across_one_hundred_fires() {
    let script = r#"
        local stream = worldrunner.random.stream("growth")
        function on_tick(dt, frame)
            worldrunner.log.info("tick", {
                frame = frame,
                roll = stream:next_float(),
                roll_int = stream:next_integer(1, 100),
            })
        end
    "#;
    let run_hundred = || {
        let mut host = host_with(script);
        for frame in 0..100 {
            host.fire("on_tick", &HookContext { dt: 0.016, frame })
                .expect("fire ok");
        }
        serde_json::to_string(&host.take_records()).expect("records serialize")
    };
    let first = run_hundred();
    let second = run_hundred();
    assert_eq!(first, second, "same seed must give byte-identical traces");
    assert!(first.contains("roll"), "trace must carry random draws");
}

#[test]
fn stale_generation_handles_report_invalid() {
    let world = Arc::new(TestWorld {
        generation: AtomicU64::new(1),
    });
    let mut host = ScriptHost::new(ScriptConfig::default()).expect("host builds");
    host.install_world_api().expect("world api installs");
    host.set_world(Some(Arc::clone(&world) as Arc<dyn WorldView>));
    host.load_module(
        "handles",
        r#"
        held = worldrunner.world.entities_by_family("trees")[1]
        function on_tick(dt, frame) return held:family() end
        "#,
    )
    .expect("loads");

    host.fire("on_tick", &HookContext::default())
        .expect("live handle works");
    world.generation.store(2, Ordering::Relaxed);
    match host.fire("on_tick", &HookContext::default()) {
        Err(ScriptError::Runtime { message, .. }) => {
            assert!(message.contains("handle invalid"), "got: {message}");
        }
        other => panic!("expected handle invalid, got {other:?}"),
    }
}

#[test]
fn missing_hooks_are_noops_not_errors() {
    let mut host = host_with("return 1");
    let result = host
        .fire("on_world_unload", &HookContext::default())
        .expect("fires");
    assert_eq!(result, worldrunner_script::HookResult::NoHook);
}

#[test]
fn api_version_is_reported_as_the_host_constant() {
    let mut host = host_with("return 1");
    let version = host
        .eval_json("worldrunner.api_version()")
        .expect("evaluates");
    assert_eq!(version, serde_json::json!("0.1.0"));
}

#[test]
fn wall_clock_is_absent() {
    let mut host = host_with("return 1");
    let error = host.eval_json("os.clock()").expect_err("no os.clock");
    let message = error.to_string();
    assert!(
        message.contains("os") || message.contains("nil"),
        "clock attempt must fail loudly: {message}"
    );
}
