use serde_json::json;

use super::{config, fault, fixture, isolate};
use crate::CompatibilityHost;

#[test]
fn actual_cdecl_contacts_fault_preserves_report_and_real_queue() {
    if !isolate(
        "compatibility::terminal_tests::controls::actual_cdecl_contacts_fault_preserves_report_and_real_queue",
    ) {
        return;
    }
    let (config, assembly) = config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, objects) = fixture(&host, world, &assembly);
    for object in &objects {
        host.request(world, &json!({"op":"component.add","object":object,"kind":"BoxCollider2D","fields":{"is_trigger":true}})).unwrap();
        host.request(world, &json!({"op":"component.add","object":object,"kind":"Rigidbody2D","fields":{"gravity_scale":0}})).unwrap();
    }
    host.load_fixture(&manifest).unwrap();
    fault::install(&host);
    fault::CONTACT.set(host.bridge.contact).unwrap();
    host.bridge.contact = fault::malformed_contacts;
    let report = host.advance_rational_report(world, 3, 100);
    let error = report.status.as_ref().unwrap_err().to_string();
    assert!(error.contains("contact"), "{error}");
    assert!(
        !report.contacts().is_empty(),
        "fixture did not generate native contacts"
    );
    let actual = host
        .native
        .request_text(world, &json!({"op":"events.read","clear":false}))
        .unwrap();
    assert_eq!(report.raw_contact_responses(), &[actual]);
    assert_eq!(
        fault::state(&host, world),
        fault::ACCEPTED.lock().unwrap().clone().unwrap()
    );
    assert!(host.measured_empty().unwrap());
    let status = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    assert_eq!(status["fixed_tick"], 1);
    assert_eq!(status["stage"], "Closed");
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

#[test]
fn actual_cdecl_initialize_transport_fault_preserves_primary_and_drains() {
    if !isolate(
        "compatibility::terminal_tests::controls::actual_cdecl_initialize_transport_fault_preserves_primary_and_drains",
    ) {
        return;
    }
    let (config, assembly) = config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, _) = fixture(&host, world, &assembly);
    fault::install(&host);
    fault::LOAD.set(host.bridge.load).unwrap();
    host.bridge.load = fault::after_load;
    let error = host.load_fixture(&manifest).unwrap_err().to_string();
    assert!(error.contains("-78"), "{error}");
    assert!(host.measured_empty().unwrap());
    assert_eq!(
        fault::state(&host, world),
        fault::ACCEPTED.lock().unwrap().clone().unwrap()
    );
    assert_eq!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap()["stage"],
        "Closed"
    );
    host.destroy_world(world).unwrap();
}

#[test]
fn actual_cdecl_closed_retains_ownership_and_refuses_reload_or_retry() {
    if !isolate(
        "compatibility::terminal_tests::controls::actual_cdecl_closed_retains_ownership_and_refuses_reload_or_retry",
    ) {
        return;
    }
    let (config, assembly) = config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, _) = fixture(&host, world, &assembly);
    host.load_fixture(&manifest).unwrap();
    host.close_phase_session(world).unwrap();
    let before = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert!(
        host.unload()
            .unwrap_err()
            .to_string()
            .contains("cleanup_unavailable_closed")
    );
    assert_eq!(
        host.observe(&json!({"op":"managed.statistics"})).unwrap(),
        before
    );
    assert!(host.load_fixture(&manifest).is_err());
    assert!(
        host.unload()
            .unwrap_err()
            .to_string()
            .contains("already attempted")
    );
    assert_eq!(host.loaded_world, Some(world));
    assert!(host.destroy_world(world).is_err());
    assert_eq!(host.store_statistics().unwrap().0, 1);
    // Drop destroys native ownership without asserting CLR teardown; process isolation owns the remaining roots.
}

#[test]
fn terminal_requirements_reject_unpaired_capabilities() {
    let fixture = json!({"requirements":{"native_capabilities":["native_terminal_cleanup_v1"],
        "bridge_capabilities":[],"execution_profile":"native_phase_context_v1"}});
    let error = super::super::terminal::required(&fixture).unwrap_err();
    assert!(error.to_string().contains("paired"));
}

#[test]
fn actual_cdecl_dispatch_guard_rejects_takeover_before_native_admission() {
    if !isolate(
        "compatibility::terminal_tests::controls::actual_cdecl_dispatch_guard_rejects_takeover_before_native_admission",
    ) {
        return;
    }
    let (config, assembly) = config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, _) = fixture(&host, world, &assembly);
    host.load_fixture(&manifest).unwrap();
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    let dispatch = host.bridge.enter_dispatch().unwrap();
    assert!(host.bridge.require_idle().is_err());
    assert!(
        host.bridge
            .invoke(host.bridge.phase, &json!({"phase":"Cleanup"}))
            .is_err()
    );
    assert_eq!(
        host.request(world, &json!({"op":"session.status"}))
            .unwrap(),
        before
    );
    drop(dispatch);
    let _ = host.unload();
    assert!(host.measured_empty().unwrap());
    host.destroy_world(world).unwrap();
}
