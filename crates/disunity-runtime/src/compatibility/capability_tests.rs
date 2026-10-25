use super::CompatibilityHost;
use serde_json::{Value, json};

fn without_capability(host: &mut CompatibilityHost, capability: &str) {
    host.negotiation.native["capabilities"]
        .as_array_mut()
        .unwrap()
        .retain(|value| value != capability);
}

fn snapshot(host: &CompatibilityHost, world: u64) -> Value {
    host.request(world, &json!({"op":"snapshot"})).unwrap()
}

#[test]
fn byte_descriptor_rejection_precedes_native_mutation() {
    if !crate::test_support::child::isolated(
        "compatibility::capability_tests::byte_descriptor_rejection_precedes_native_mutation",
    ) {
        return;
    }
    let mut host = CompatibilityHost::boot(&crate::test_support::artifacts().config()).unwrap();
    let world = host.create_world().unwrap();
    let before = snapshot(&host, world);
    without_capability(&mut host, "transport_byte_limits_v1");
    let error = host.select_byte_profile(world).unwrap_err().to_string();
    assert!(error.contains("capability"), "{error}");
    assert_eq!(snapshot(&host, world), before);
    assert_eq!(
        host.request(world, &json!({"version":1,"op":"transport.get"}))
            .unwrap()["transport_profile"],
        "legacy"
    );
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn nested_descriptor_rejection_precedes_native_mutation() {
    if !crate::test_support::child::isolated(
        "compatibility::capability_tests::nested_descriptor_rejection_precedes_native_mutation",
    ) {
        return;
    }
    let mut host = CompatibilityHost::boot(&crate::test_support::artifacts().config()).unwrap();
    let world = host.create_world().unwrap();
    let before = snapshot(&host, world);
    without_capability(&mut host, "native_readiness_nested_start_v1");
    assert!(host.open_nested_phase_session(world).is_err());
    assert_eq!(snapshot(&host, world), before);
    assert!(!host.strict.contains_key(&world));
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn world_descriptor_mismatch_precedes_selection_and_external_adoption() {
    if !crate::test_support::child::isolated(
        "compatibility::capability_tests::world_descriptor_mismatch_precedes_selection_and_external_adoption",
    ) {
        return;
    }
    let mut host = CompatibilityHost::boot(&crate::test_support::artifacts().config()).unwrap();
    let owned = host.create_world().unwrap();
    let external = host.native.create_world().unwrap();
    host.negotiation.native["limits"]["max_request_bytes"] = json!(1);
    for world in [owned, external] {
        let before = snapshot(&host, world);
        let fixture =
            json!({"world":world,"requirements":{"execution_profile":"native_phase_context_v1"}});
        let error = host.load_fixture(&fixture).unwrap_err().to_string();
        assert!(error.contains("world descriptor"), "{error}");
        assert_eq!(snapshot(&host, world), before);
        assert!(!host.strict.contains_key(&world));
    }
    assert!(!host.worlds.contains(&external));
    host.native.destroy_world(external).unwrap();
    host.destroy_world(owned).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn created_world_negotiation_failure_releases_native_world() {
    if !crate::test_support::child::isolated(
        "compatibility::capability_tests::created_world_negotiation_failure_releases_native_world",
    ) {
        return;
    }
    let mut host = CompatibilityHost::boot(&crate::test_support::artifacts().config()).unwrap();
    host.negotiation.native["limits"]["max_request_bytes"] = json!(1);
    assert!(host.create_world().is_err());
    assert_eq!(host.store_statistics().unwrap().0, 0);
    assert!(host.worlds.is_empty());
}
