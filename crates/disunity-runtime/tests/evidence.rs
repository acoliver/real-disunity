#[path = "support/phase_contacts.rs"]
mod contacts;
#[path = "support/handshake.rs"]
mod support;

use serde_json::{Value, json};
use worldrunner_disunity_runtime::{
    AdvanceReport, CapturedSnapshot, CompatibilityHost, ManagedHostConfig,
};

fn request(host: &CompatibilityHost, world: u64, value: Value) -> Value {
    host.request(world, &value).unwrap()
}

#[test]
fn actual_native_reports_and_authenticated_snapshot_receipts() {
    let a = support::artifacts();
    let config = ManagedHostConfig {
        ffi_library: a.native.clone(),
        runtime_config: a.bridge().with_extension("runtimeconfig.json"),
        assembly: a.bridge(),
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    };
    let mut host = CompatibilityHost::boot(&config).unwrap();
    contacts::numeric_runtime_frame(&mut host, &a.probe());
    contacts::runtime_frame(&mut host, &a.probe());
    journal(&mut host);
    snapshots(&mut host);
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

fn journal(host: &mut CompatibilityHost) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let (manifest, _) = contacts::fixture(host, world, &support::artifacts().probe());
    host.load_fixture(&manifest).unwrap();
    let report: AdvanceReport = host.advance_rational_report(world, 1, 25);
    assert!(
        report
            .status
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("solver contact sentinel")
    );
    assert_eq!(report.raw_contact_responses().len(), 2);
    assert!(!report.contacts().is_empty());
    let mut expected = Vec::new();
    for raw in report.raw_contact_responses() {
        let envelope: Value = serde_json::from_str(raw).unwrap();
        expected.extend(envelope["data"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(report.contacts(), expected);
    assert_eq!(
        request(host, world, json!({"op":"events.read","clear":false})),
        json!([])
    );
    let status = request(host, world, json!({"op":"session.status"}));
    assert_eq!(status["stage"], "Idle");
    assert_eq!(status["fixed_tick"], 2);
    let held = report.contacts().to_vec();
    let quiet = host.advance_rational_report(world, 1, 100);
    quiet.status.as_ref().unwrap_err();
    assert!(quiet.contacts().is_empty());
    assert!(quiet.raw_contact_responses().is_empty());
    assert_eq!(report.contacts(), held);
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
}

fn snapshot_world(host: &mut CompatibilityHost) -> (u64, u64) {
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    rejected(host.capture_snapshot(world));
    let (manifest, peer) = contacts::fixture(host, world, &support::artifacts().probe());
    request(
        host,
        world,
        json!({"op":"transform.set","object":peer,"position":[-0.36050745844841003,0.05,0.0]}),
    );
    host.load_fixture(&manifest).unwrap();
    (world, peer)
}

fn snapshots(host: &mut CompatibilityHost) {
    let (world, peer) = snapshot_world(host);
    let supported = request(host, world, json!({"op":"describe"}))["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("snapshot_primitives_v1"));
    if !supported {
        let before = request(host, world, json!({"op":"session.status"}));
        rejected(host.capture_snapshot(world));
        assert_eq!(before, request(host, world, json!({"op":"session.status"})));
        host.unload().unwrap();
        host.destroy_world(world).unwrap();
        return;
    }
    let capture: CapturedSnapshot = host.capture_snapshot(world).unwrap();
    assert_eq!(capture.stamp().world(), world);
    assert!(host.validate_snapshot(world, &capture, true).unwrap());
    let raw = capture.raw_snapshot().to_owned();
    assert!(
        raw.contains("-0.36050745844841003"),
        "missing real f32 witness: {raw}"
    );
    let objects = capture.value()["objects"].as_array().unwrap();
    let object = objects
        .iter()
        .find(|o| o["handle"].as_u64() == Some(peer))
        .unwrap();
    assert_eq!(
        object["transform"]["position"][0]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.36050745844841003_f64).to_bits()
    );
    rejected(host.advance_rational(world, 1, 50));
    rejected(host.validate_snapshot(world, &capture, true));
    assert!(!host.validate_snapshot(world, &capture, false).unwrap());
    let newer = host.capture_snapshot(world).unwrap();
    assert_ne!(
        newer.stamp().content_revision(),
        capture.stamp().content_revision()
    );
    rejected(host.advance_rational(world, 1, 50));
    rejected(host.validate_snapshot(world, &newer, true));
    rejected(host.advance_rational(world, 1, 100));
    assert!(!host.validate_snapshot(world, &capture, false).unwrap());
    assert_eq!(capture.raw_snapshot(), raw);
    let other = host.create_world().unwrap();
    rejected(host.validate_snapshot(other, &capture, false));
    host.destroy_world(other).unwrap();
    host.release_snapshot(world, &capture).unwrap();
    rejected(host.release_snapshot(world, &capture));
    rejected(host.validate_snapshot(world, &capture, false));
    host.release_snapshot(world, &newer).unwrap();
    host.unload().unwrap();
    rejected(host.capture_snapshot(world));
    host.destroy_world(world).unwrap();
    rejected(host.validate_snapshot(world, &capture, false));
    assert_eq!(capture.raw_snapshot(), raw);
}

fn rejected<T, E>(result: Result<T, E>) {
    assert!(result.is_err());
}
