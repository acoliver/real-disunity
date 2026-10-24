use super::{CompatibilityHost, Value, control, fixture, json, phase};

pub fn failing_callbacks(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (mut manifest, object) = fixture(host, world, false);
    manifest["graph"]["components"][0]["script"]["fields"]["failCleanup"] = json!(true);
    host.load_fixture(&manifest).unwrap();
    host.advance_rational(world, 1, 30).unwrap();
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    let error = host.unload().unwrap_err().to_string();
    assert!(error.contains("strict Dispose sentinel"), "{error}");
    let after = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(after["stage"], "Closed");
    assert_eq!(after["frame"], before["frame"]);
    assert_eq!(after["fixed_tick"], before["fixed_tick"]);
    assert!(after["active_phase"].is_null());
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"],
        json!([11., 10., 10.])
    );
    let failures = host.callback_failures(0).unwrap();
    assert_eq!(failures.total, 3);
    for failure in failures.records {
        assert_eq!(failure.phase, "Cleanup");
        assert_eq!(failure.frame, before["frame"].as_u64());
    }
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0, "failing cleanup retained {key}");
    }
    assert!(
        host.request(
            world,
            &json!({"op":"transform.set","object":object,"position":[99,99,99],"space":"world"})
        )
        .is_err()
    );
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

pub fn cleanup_without_frame(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, object) = fixture(host, world, false);
    host.load_fixture(&manifest).unwrap();
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    host.unload().unwrap();
    let after = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(
        after["frame"], before["frame"],
        "unload advanced gameplay frame"
    );
    assert_eq!(after["fixed_tick"], before["fixed_tick"]);
    assert_eq!(after["stage"], "Closed");
    assert!(after["active_phase"].is_null());
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"],
        json!([1., 10., 10.])
    );
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(roots["data"][key], 0, "cleanup retained {key}");
    }
    host.destroy_world(world).unwrap();
}

pub fn manual_unload(host: &mut CompatibilityHost, world: u64, authority: &Value) {
    let mut status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    while let Some(name) = status["stage"]["Phase"].as_str() {
        let context = phase(host, world, authority, name);
        let _ = host.dispatch_phase(&json!({"phase":name,"context":context}));
        control(
            host,
            world,
            authority,
            json!({"op":"phase.finish","context":context}),
        );
        status = host
            .request(world, &json!({"op":"session.status"}))
            .unwrap();
    }
    if status["stage"] == "End" {
        control(
            host,
            world,
            authority,
            json!({"op":"frame.end","frame":status["frame"]}),
        );
    }
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    let context = phase(host, world, authority, "Cleanup");
    let native = host.request(world, &json!({"op":"snapshot"})).unwrap();
    for bad in [
        json!({"phase":"Cleanup"}),
        json!({"phase":"Update","context":context,"cleanup":true}),
    ] {
        assert!(host.dispatch_phase(&bad).is_err());
        assert_eq!(
            host.request(world, &json!({"op":"snapshot"})).unwrap(),
            native
        );
    }
    let _ = host.dispatch_phase(&json!({"phase":"Cleanup","context":context}));
    control(
        host,
        world,
        authority,
        json!({"op":"phase.finish","context":context}),
    );
    let after = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(after["frame"], before["frame"]);
    assert_eq!(after["fixed_tick"], before["fixed_tick"]);
    assert_eq!(after["stage"], "Closed");
    host.unload().unwrap();
}
