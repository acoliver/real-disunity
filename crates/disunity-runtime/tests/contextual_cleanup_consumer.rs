#[path = "contextual_cleanup_helpers/assertions.rs"]
mod assertions;
#[path = "contextual_cleanup_helpers/fixture.rs"]
mod fixture;
#[path = "support/handshake.rs"]
pub mod support;

use serde_json::json;
use worldrunner_disunity_runtime::CompatibilityHost;

#[test]
fn actual_host_contextual_cleanup_closes_after_callback_failures_and_releases_roots() {
    let config = fixture::config();
    // Native TLS and CLR roots may survive an assertion failure until process exit.
    static IMAGE: std::sync::OnceLock<libloading::Library> = std::sync::OnceLock::new();
    IMAGE.get_or_init(|| unsafe { libloading::Library::new(&config.ffi_library).unwrap() });
    let mut host = CompatibilityHost::boot(&config).unwrap();
    assertions::successful_cleanup(&mut host);
    let world = host.create_world().unwrap();
    host.open_contextual_cleanup_phase_session(world).unwrap();
    let manifest = fixture::manifest(&host, world, "ContextualCleanupProbe", 2048);
    assertions::selection_controls(&mut host, world, &manifest);
    host.load_fixture(&manifest).unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 2048);
    let before = assertions::preserved_state(&host, world);
    assertions::unsupported_operations(&mut host, world);
    let error = host.unload().unwrap_err().to_string();
    assert!(
        error.contains("managed callback failures: total=43"),
        "{error}"
    );
    assertions::failures(&host);
    assertions::closed_and_empty(&mut host, world, &before);
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    println!(
        "PASS actual Rust CompatibilityHost: 2048 probe bindings; 43 retained CLR failures; Closed; measured roots=0; loaded ownership cleared; native world explicitly destroyed"
    );
}
