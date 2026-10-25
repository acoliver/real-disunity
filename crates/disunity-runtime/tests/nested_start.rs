mod nested_support;
#[path = "support/handshake.rs"]
pub mod support;

use nested_support::{config, fixture};
use serde_json::{Value, json};
use worldrunner_disunity_runtime::CompatibilityHost;

/// A process admits one live `CompatibilityHost`, and each test here boots its own.
static ONE_HOST: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_host_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_HOST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn actual_cdecl_nested_host_matches_before_initialize_and_drains() {
    let _host_slot = one_host_at_a_time();
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();
    host.open_nested_phase_session(world).unwrap();
    assert!(host.open_phase_session(world).is_err());
    let manifest = fixture(&host, world, 0, 3);
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    for requirement in [Value::Null, json!("native_readiness_single_v1")] {
        let mut bad = manifest.clone();
        bad["requirements"]["scheduler_profile"] = requirement;
        assert!(host.load_fixture(&bad).is_err());
        assert_eq!(
            host.request(world, &json!({"op":"session.status"}))
                .unwrap(),
            before
        );
    }
    host.load_fixture(&manifest).unwrap();
    host.advance_rational(world, 0, 1).unwrap();
    let snapshot = host.observe(&json!({"op":"managed.snapshot"})).unwrap();
    let fields = &snapshot["data"]["instances"][0]["fields"];
    assert_eq!(fields["moves"], 3);
    assert_eq!(fields["disposals"], 3);
    assert_eq!(fields["after"], 1);
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn actual_cdecl_nested_claim_budget_is_shared_and_recovery_drains() {
    let _host_slot = one_host_at_a_time();
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();
    host.open_nested_phase_session(world).unwrap();
    host.load_fixture(&fixture(&host, world, 50, 1)).unwrap();
    host.advance_rational(world, 0, 1).unwrap();
    let snapshot = host.observe(&json!({"op":"managed.snapshot"})).unwrap();
    let fields = &snapshot["data"]["instances"][0]["fields"];
    let moves = fields["moves"].as_u64().unwrap();
    assert!((1000..2048).contains(&moves));
    assert_eq!(fields["moves"], fields["disposals"]);
    assert_eq!(fields["rejected"], 1);
    assert_eq!(fields["after"], 1);
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn actual_cdecl_nested_claim_peer_exhaustion_closes_without_root_bypass() {
    let _host_slot = one_host_at_a_time();
    let mut host = CompatibilityHost::boot(&config()).unwrap();
    let world = host.create_world().unwrap();
    host.open_nested_phase_session(world).unwrap();
    let mut manifest = fixture(&host, world, 50, 1);
    nested_support::add_peer(&host, world, &mut manifest);
    host.load_fixture(&manifest).unwrap();
    let error = host.advance_rational(world, 0, 1).unwrap_err().to_string();
    assert!(
        error.contains("readiness root claim budget exceeded"),
        "{error}"
    );
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Closed");
    assert_eq!(
        status["terminal_recovery"]["native_obligations"]["remaining_routines"],
        0
    );
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0);
    }
    assert!(host.advance_rational(world, 0, 1).is_err());
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}
