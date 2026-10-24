use serde_json::{Value, json};
use worldrunner_disunity_runtime::CompatibilityHost;

pub fn successful_cleanup(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    host.open_contextual_cleanup_phase_session(world).unwrap();
    let manifest = super::fixture::manifest(host, world, "HandshakeProbe", 1);
    host.load_fixture(&manifest).unwrap();
    let owner = manifest["handles"]["owner"].as_u64().unwrap();
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":owner}))
            .unwrap()["position"],
        json!([99., 0., 0.])
    );
    let before = preserved_state(host, world);
    host.unload().unwrap();
    closed_and_empty(host, world, &before);
    host.destroy_world(world).unwrap();
    println!("PASS actual Rust success path: Awake native write=99; unload Ok; Closed; roots=0");
}

pub fn selection_controls(host: &mut CompatibilityHost, world: u64, manifest: &Value) {
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    for field in ["execution_profile", "scheduler_profile", "callback_profile"] {
        let mut bad = manifest.clone();
        bad["requirements"].as_object_mut().unwrap().remove(field);
        assert!(host.load_fixture(&bad).is_err(), "missing {field} accepted");
        assert_eq!(
            host.request(world, &json!({"op":"session.status"}))
                .unwrap(),
            before
        );
    }
    for (field, capability) in [
        ("native_capabilities", "native_terminal_cleanup_v1"),
        ("bridge_capabilities", "managed_terminal_cleanup_v1"),
    ] {
        let mut bad = manifest.clone();
        bad["requirements"][field]
            .as_array_mut()
            .unwrap()
            .push(json!(capability));
        let error = host.load_fixture(&bad).unwrap_err().to_string();
        assert!(
            error.contains("terminal recovery composition is unsupported"),
            "{error}"
        );
        assert_eq!(
            host.request(world, &json!({"op":"session.status"}))
                .unwrap(),
            before
        );
    }
}

pub fn unsupported_operations(host: &mut CompatibilityHost, world: u64) {
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert!(host.advance_rational(world, 1, 30).is_err());
    assert!(host.dispatch_phase(&json!({"phase":"Update"})).is_err());
    assert!(host.invoke_contact(&json!({"events":[]})).is_err());
    assert_eq!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap(),
        before
    );
}

pub fn preserved_state(host: &CompatibilityHost, world: u64) -> Value {
    json!({"time":host.request(world, &json!({"op":"time.get"})).unwrap(),
        "input":host.request(world, &json!({"op":"input.get"})).unwrap(),
        "input_status":host.request(world, &json!({"op":"input.status"})).unwrap()})
}

pub fn closed_and_empty(host: &mut CompatibilityHost, world: u64, before: &Value) {
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Closed");
    assert_eq!(status["frame"], 0);
    assert_eq!(status["fixed_tick"], 0);
    assert_eq!(preserved_state(host, world), *before);
    assert_eq!(before["time"]["time"], 0.);
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for field in ["scripts", "bindings", "coroutines"] {
        assert_eq!(roots["data"][field], 0, "retained {field}: {roots}");
    }
    assert!(roots["data"]["world"].is_null());
    assert_eq!(roots["data"]["loaded"], false);
    assert_eq!(roots["data"]["cleanup_pending"], false);
    assert!(host.observe(&json!({"op":"managed.snapshot"})).is_err());
    host.unload().unwrap();
    assert!(host.cleanup_contextual(world).is_err());
    println!("MEASURED status={status} managed={roots} preserved={before}");
}

pub fn failures(host: &CompatibilityHost) {
    let mut after = 0;
    let mut indices = Vec::new();
    loop {
        let page = host.callback_failures(after).unwrap();
        assert_eq!((page.total, page.retained, page.dropped), (43, 43, 0));
        for failure in &page.records {
            assert_eq!(failure.script_type, "ContextualCleanupProbe");
            assert_eq!(failure.phase, "Cleanup");
            assert_eq!(failure.exception_type, "System.ArgumentException");
            assert!(matches!(
                failure.callback.as_str(),
                "OnDisable" | "OnDestroy"
            ));
            let index = failure
                .message
                .strip_prefix("contextual Cleanup actual CLR failure ")
                .unwrap()
                .parse::<u64>()
                .unwrap();
            indices.push(index);
        }
        if !page.has_more {
            break;
        }
        assert!(page.next > after);
        after = page.next;
    }
    assert_eq!(indices, (0..4096).step_by(97).collect::<Vec<_>>());
    println!("MEASURED retained actual CLR exception entry indices={indices:?}");
}
