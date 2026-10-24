#[path = "support/phase_cleanup.rs"]
mod cleanup;
use cleanup::{cleanup_without_frame, manual_unload};
#[path = "support/phase_contacts.rs"]
mod contacts;
#[path = "support/phase_envelope.rs"]
mod envelope;
#[path = "support/phase_privacy.rs"]
mod privacy;
#[path = "support/phase_bridge.rs"]
mod raw;
#[path = "support/handshake.rs"]
mod support;

use serde_json::{Value, json};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

const PROFILE: &str = "native_phase_context_v1";

fn config() -> ManagedHostConfig {
    let a = support::artifacts();
    ManagedHostConfig {
        ffi_library: a.native.clone(),
        runtime_config: a.bridge().with_extension("runtimeconfig.json"),
        assembly: a.bridge(),
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    }
}

fn fixture(host: &CompatibilityHost, world: u64, fail: bool) -> (Value, u64) {
    let object = host
        .request(world, &json!({"op":"object.create","name":"strict-probe"}))
        .unwrap()["handle"]
        .as_u64()
        .unwrap();
    let script = host.request(world, &json!({"op":"component.add","object":object,"kind":"Script","fields":{"type":"StrictPhaseProbe"}})).unwrap()["handle"].as_u64().unwrap();
    (
        json!({"world":world,"assemblies":[support::artifacts().probe()],
        "requirements":{"execution_profile":PROFILE},"handles":{"owner":object,"script":script},
        "graph":{"nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
        "components":[{"id":"script","object":"owner","script":{"type_name":"StrictPhaseProbe","fields":{"failUpdate":fail}}}]}}),
        object,
    )
}

fn fields(host: &CompatibilityHost) -> Value {
    host.observe(&json!({"op":"managed.snapshot"})).unwrap()["data"]["instances"][0]["fields"]
        .clone()
}

fn control(host: &CompatibilityHost, world: u64, authority: &Value, mut request: Value) -> Value {
    request["host"] = authority.clone();
    host.request(world, &request).unwrap()
}

fn phase(host: &CompatibilityHost, world: u64, authority: &Value, name: &str) -> Value {
    let s = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    let tick = s["fixed_tick"].as_u64().unwrap()
        + u64::from(matches!(name, "FixedUpdate" | "FixedCoroutines"));
    control(
        host,
        world,
        authority,
        json!({"op":"phase.open","phase":name,"frame":s["frame"],"fixed_tick":tick}),
    )["context"]
        .clone()
}

#[test]
fn real_cdecl_strict_context_controls_and_complete_runtime_frame() {
    let config = config();
    static IMAGE: std::sync::OnceLock<libloading::Library> = std::sync::OnceLock::new();
    // CLR and native TLS can outlive an early-rejected host. Keep this test image mapped.
    IMAGE.get_or_init(|| unsafe { libloading::Library::new(&config.ffi_library).unwrap() });
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let raw = raw::RawBridge::bind(&config);
    contacts::numeric_runtime_frame(&mut host, &support::artifacts().probe());
    cleanup_without_frame(&mut host);
    cleanup::failing_callbacks(&mut host);
    initialize_controls(&mut host, &raw);
    contact_controls(&mut host, false);
    contact_controls(&mut host, true);
    envelope::controls(&mut host, &raw);
    contacts::runtime_frame(&mut host, &support::artifacts().probe());
    runtime_frame(&mut host, false);
    runtime_frame(&mut host, true);
    binding_failure(&mut host);
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn initialize_controls(host: &mut CompatibilityHost, raw: &raw::RawBridge) {
    let world = host.create_world().unwrap();
    let (mut manifest, object) = fixture(host, world, false);
    let opened = host
        .request(world, &json!({"op":"session.open","profile":PROFILE}))
        .unwrap();
    let authority = json!({"session":opened["session"],"host_key":opened["host_key"]});
    let context = phase(host, world, &authority, "Initialize");
    invalid_initial(host, raw, &mut manifest, &context);
    host.load_fixture(&manifest).unwrap();

    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"][0],
        1.
    );
    control(
        host,
        world,
        &authority,
        json!({"op":"phase.finish","context":context}),
    );
    privacy::public_evidence(host, world, &authority);
    dispatch_controls(host, world, &authority);
    fixed_context_controls(host, world, &authority);
    let before = fields(host);
    assert_ne!(
        raw.unload(),
        0,
        "uncredentialed unload executed user cleanup"
    );
    assert_eq!(fields(host), before);
    manual_unload(host, world, &authority);
    assert_ne!(
        raw.load(&manifest),
        0,
        "stale Initialize replay accepted after unload"
    );
    host.destroy_world(world).unwrap();
    stale_generation(host, raw, &manifest);
}

fn invalid_initial(
    host: &CompatibilityHost,
    raw: &raw::RawBridge,
    manifest: &mut Value,
    context: &Value,
) {
    let world = manifest["world"].as_u64().unwrap();
    let before = host.request(world, &json!({"op":"snapshot"})).unwrap();
    for field in [
        "world",
        "frame",
        "fixed_tick",
        "sequence",
        "phase",
        "session",
        "token",
    ] {
        let mut bad = context.clone();
        bad[field] = if bad[field].is_number() {
            json!(bad[field].as_u64().unwrap() + 1)
        } else {
            json!("invalid")
        };
        manifest["initial_context"] = bad;
        assert_ne!(raw.load(manifest), 0, "accepted wrong {field}");
        assert_eq!(
            host.request(world, &json!({"op":"snapshot"})).unwrap(),
            before
        );
        let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
        assert_eq!(roots["data"]["bindings"], 0);
    }
    manifest.as_object_mut().unwrap().remove("initial_context");
    assert_ne!(raw.load(manifest), 0);
    manifest["initial_context"] = json!({"token":context["token"]});
    assert_ne!(raw.load(manifest), 0, "bare token accepted");
    manifest["initial_context"] = context.clone();
    let mut missing_profile = manifest.clone();
    missing_profile
        .as_object_mut()
        .unwrap()
        .remove("requirements");
    assert_ne!(
        raw.load(&missing_profile),
        0,
        "strict session silently downgraded"
    );
}

fn dispatch_controls(host: &mut CompatibilityHost, world: u64, authority: &Value) {
    control(
        host,
        world,
        authority,
        json!({"op":"frame.open","dt":{"num":1,"den":100}}),
    );
    let context = phase(host, world, authority, "Update");
    let before = fields(host);
    for bad in [
        json!({"phase":"Update"}),
        json!({"phase":"Update","context":{"token":context["token"]}}),
        json!({"phase":"FixedUpdate","context":context}),
    ] {
        assert!(host.dispatch_phase(&bad).is_err());
        assert_eq!(fields(host), before);
    }
    let good = json!({"phase":"Update","context":context});
    host.dispatch_phase(&good).unwrap();
    assert_eq!(fields(host)["updates"], 1);
    assert!(host.dispatch_phase(&good).is_err());
    assert_eq!(fields(host)["updates"], 1);
    control(
        host,
        world,
        authority,
        json!({"op":"phase.finish","context":context}),
    );
    for name in ["UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        let context = phase(host, world, authority, name);
        let before = fields(host);
        assert!(host.dispatch_phase(&json!({"phase":name})).is_err());
        assert_eq!(fields(host), before);
        let request = json!({"phase":name,"context":context});
        host.dispatch_phase(&request).unwrap();
        let after = fields(host);
        assert!(host.dispatch_phase(&request).is_err());
        assert_eq!(fields(host), after);
        control(
            host,
            world,
            authority,
            json!({"op":"phase.finish","context":context}),
        );
    }
    control(host, world, authority, json!({"op":"frame.end","frame":1}));
}

fn stale_generation(host: &mut CompatibilityHost, raw: &raw::RawBridge, previous: &Value) {
    let world = host.create_world().unwrap();
    let (mut manifest, _) = fixture(host, world, false);
    let opened = host
        .request(world, &json!({"op":"session.open","profile":PROFILE}))
        .unwrap();
    let authority = json!({"session":opened["session"],"host_key":opened["host_key"]});
    let context = phase(host, world, &authority, "Initialize");
    manifest["initial_context"] = previous["initial_context"].clone();
    let before = host.request(world, &json!({"op":"snapshot"})).unwrap();
    assert_ne!(raw.load(&manifest), 0, "cross-generation context accepted");
    assert_eq!(
        host.request(world, &json!({"op":"snapshot"})).unwrap(),
        before
    );
    manifest["initial_context"] = context.clone();
    host.load_fixture(&manifest).unwrap();
    control(
        host,
        world,
        &authority,
        json!({"op":"phase.finish","context":context}),
    );
    manual_unload(host, world, &authority);
    host.destroy_world(world).unwrap();
}

fn runtime_frame(host: &mut CompatibilityHost, fail: bool) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, object) = fixture(host, world, fail);
    host.request(world, &json!({"op":"input.events","events":[{"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true}]})).unwrap();
    host.load_fixture(&manifest).unwrap();
    let advanced = host.advance_rational(world, 1, 30);
    assert_eq!(advanced.is_err(), fail);
    let f = fields(host);
    assert_first_frame(&f);
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["stage"], "Idle");
    assert!(status["active_phase"].is_null());
    if fail {
        let failures = host.callback_failures(0).unwrap();
        assert_eq!(failures.total, 1);
        assert_eq!(failures.records[0].phase, "Update");
        assert_eq!(failures.records[0].frame, Some(1));
    }
    assert_eq!(host.advance_rational(world, 1, 30).is_err(), fail);
    assert_second_frame(host);
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    host.unload().unwrap();
    let after = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(after["frame"], before["frame"]);
    assert_eq!(after["fixed_tick"], before["fixed_tick"]);
    assert_eq!(after["stage"], "Closed");
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":object}))
            .unwrap()["position"],
        json!([11., 10., 10.])
    );
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["bindings"], 0);
    host.destroy_world(world).unwrap();
}

fn assert_second_frame(host: &CompatibilityHost) {
    let next = fields(host);
    assert_eq!(next["updates"], 2);
    assert_eq!(next["lateUpdates"], 2);
    assert_eq!(next["updateResumes"], 1);
    assert_eq!(next["fixedResumes"], 1);
    assert_eq!(next["endResumes"], 1);
}

fn assert_first_frame(f: &Value) {
    assert_eq!(f["updates"], 1);
    assert_eq!(
        f["lateUpdates"], 1,
        "callback failure must not skip LateUpdate"
    );
    assert_eq!(
        f["endResumes"], 1,
        "EndOfFrame must run once after LateUpdate"
    );
    assert_eq!(f["fixedResumes"], 1);
    assert_eq!(f["settersRejected"], true);
    assert_eq!(f["bypassRejected"], true);
    assert_eq!(f["fixedHeld"], true);
    assert_eq!(f["fixedDown"], true);
    assert_eq!(f["updateHeld"], true);
    assert!((f["fixedDelta"].as_f64().unwrap() - 0.02).abs() < 1e-6);
    assert!((f["unscaled"].as_f64().unwrap() - 0.02).abs() < 1e-6);
    assert!((f["dynamicDelta"].as_f64().unwrap() - 1.0 / 30.0).abs() < 1e-6);
}

fn prepare_contacts(
    host: &mut CompatibilityHost,
    world: u64,
    compound: bool,
) -> (Value, u64, Value) {
    let (mut manifest, peer) = contacts::fixture(host, world, &support::artifacts().probe());
    if compound {
        host.request(
            world,
            &json!({"op":"component.add","object":peer,
            "kind":"CircleCollider2D","fields":{"is_trigger":true}}),
        )
        .unwrap();
    }
    let opened = host
        .request(world, &json!({"op":"session.open","profile":PROFILE}))
        .unwrap();
    let authority = json!({"session":opened["session"],"host_key":opened["host_key"]});
    let initial = phase(host, world, &authority, "Initialize");
    manifest["initial_context"] = initial.clone();
    host.load_fixture(&manifest).unwrap();
    control(
        host,
        world,
        &authority,
        json!({"op":"phase.finish","context":initial}),
    );
    control(
        host,
        world,
        &authority,
        json!({"op":"frame.open","dt":{"num":1,"den":50}}),
    );
    for name in ["FixedUpdate", "FixedCoroutines"] {
        let context = phase(host, world, &authority, name);
        host.dispatch_phase(&json!({"phase":name,"context":context}))
            .unwrap();
        control(
            host,
            world,
            &authority,
            json!({"op":"phase.finish","context":context}),
        );
    }
    control(
        host,
        world,
        &authority,
        json!({"op":"fixed.commit","frame":1,"fixed_tick":1}),
    );
    let context = phase(host, world, &authority, "Contacts");
    (authority, peer, context)
}

fn contact_controls(host: &mut CompatibilityHost, tamper: bool) {
    let world = host.create_world().unwrap();
    let (authority, peer, context) = prepare_contacts(host, world, false);
    let before = host.request(world, &json!({"op":"snapshot"})).unwrap();
    assert!(host.invoke_contact(&json!({"events":[]})).is_err());
    assert_eq!(
        host.request(world, &json!({"op":"snapshot"})).unwrap(),
        before
    );
    let events = retained_contact_events(host, world, &before);
    let mut request = json!({"context":context,"events":events});
    if tamper {
        request["events"] = json!([{"normal":[99,99]}]);
    }
    let error = host.invoke_contact(&request).unwrap_err().to_string();
    assert!(
        error.contains(if tamper {
            "native event queue"
        } else {
            "solver contact sentinel"
        }),
        "{error}"
    );
    let after = host
        .request(world, &json!({"op":"transform.get","object":peer}))
        .unwrap();
    let remaining = host
        .request(world, &json!({"op":"events.read","clear":false}))
        .unwrap();
    assert_eq!(remaining, if tamper { events } else { json!([]) });
    assert_eq!(after["position"][2], if tamper { 0. } else { 1. });
    assert!(
        host.invoke_contact(&request).is_err(),
        "duplicate contact root accepted"
    );
    assert_eq!(
        host.request(world, &json!({"op":"transform.get","object":peer}))
            .unwrap(),
        after
    );
    control(
        host,
        world,
        &authority,
        json!({"op":"phase.finish","context":context}),
    );
    manual_unload(host, world, &authority);
    host.destroy_world(world).unwrap();
}

fn retained_contact_events(host: &CompatibilityHost, world: u64, before: &Value) -> Value {
    let events = host
        .request(world, &json!({"op":"events.read","clear":false}))
        .unwrap();
    assert!(
        !events.as_array().unwrap().is_empty(),
        "solver produced no contacts"
    );
    assert_eq!(
        host.request(world, &json!({"op":"snapshot"})).unwrap(),
        *before
    );
    assert!(
        host.request(world, &json!({"op":"events.read","clear":true}))
            .is_err()
    );
    events
}

fn fixed_context_controls(host: &mut CompatibilityHost, world: u64, authority: &Value) {
    control(
        host,
        world,
        authority,
        json!({"op":"frame.open","dt":{"num":1,"den":50}}),
    );
    for name in ["FixedUpdate", "FixedCoroutines"] {
        let context = phase(host, world, authority, name);
        let before = fields(host);
        let native = host.request(world, &json!({"op":"snapshot"})).unwrap();
        assert!(host.dispatch_phase(&json!({"phase":name})).is_err());
        assert_eq!(fields(host), before);
        assert_eq!(
            host.request(world, &json!({"op":"snapshot"})).unwrap(),
            native
        );
        let request = json!({"phase":name,"context":context});
        host.dispatch_phase(&request).unwrap();
        let after = fields(host);
        assert!(host.dispatch_phase(&request).is_err());
        assert_eq!(fields(host), after);
        control(
            host,
            world,
            authority,
            json!({"op":"phase.finish","context":context}),
        );
    }
    assert_eq!(fields(host)["fixedResumes"], 1);
    control(
        host,
        world,
        authority,
        json!({"op":"fixed.commit","frame":2,"fixed_tick":1}),
    );
    let context = phase(host, world, authority, "Contacts");
    host.invoke_contact(&json!({"context":context})).unwrap();
    control(
        host,
        world,
        authority,
        json!({"op":"phase.finish","context":context}),
    );
    for name in ["Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame"] {
        let context = phase(host, world, authority, name);
        host.dispatch_phase(&json!({"phase":name,"context":context}))
            .unwrap();
        control(
            host,
            world,
            authority,
            json!({"op":"phase.finish","context":context}),
        );
    }
    control(host, world, authority, json!({"op":"frame.end","frame":2}));
}

fn binding_failure(host: &mut CompatibilityHost) {
    for _ in 0..2 {
        let world = host.create_world().unwrap();
        host.open_phase_session(world).unwrap();
        let (mut manifest, object) = fixture(host, world, false);
        manifest["assemblies"] = json!(["/missing-strict-assembly.dll"]);
        let error = host.load_fixture(&manifest).unwrap_err().to_string();
        assert!(error.contains("fixture assembly not found"), "{error}");
        assert_eq!(
            host.request(world, &json!({"op":"transform.get","object":object}))
                .unwrap()["position"],
            json!([0., 0., 0.])
        );
        let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
        assert_eq!(roots["data"]["bindings"], 0);
        assert_eq!(roots["data"]["scripts"], 0);
        assert_eq!(
            host.request(world, &json!({"op":"session.status"}))
                .unwrap()["stage"],
            "Closed"
        );
        host.destroy_world(world).unwrap();
    }
}
