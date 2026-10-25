#[test]
fn actual_native_receipt_forgery_rejection() {
    if !crate::test_support::child::isolated(
        "compatibility::snapshot::tests::actual_native_receipt_forgery_rejection",
    ) {
        return;
    }
    let (mut host, world) = snapshot_host();
    let capture = host.capture_snapshot(world).unwrap();
    let before = host
        .request(world, &json!({"op":"session.status"}))
        .unwrap();
    for field in [
        "world",
        "world_generation",
        "session",
        "frame",
        "fixed_tick",
        "schedule_revision",
        "content_revision",
        "snapshot_id",
        "snapshot_sha256",
    ] {
        let mut stamp = serde_json::to_value(capture.stamp()).unwrap();
        stamp[field] = if let Some(n) = stamp[field].as_u64() {
            json!(n + 1)
        } else {
            json!("forged")
        };
        let forged = CapturedSnapshot {
            raw: RawValue::from_string(capture.raw_snapshot().to_owned()).unwrap(),
            stamp: serde_json::from_value(stamp).unwrap(),
            value: capture.value().clone(),
        };
        assert!(
            host.validate_snapshot(world, &forged, false).is_err(),
            "accepted {field}"
        );
        assert_eq!(
            before,
            host.request(world, &json!({"op":"session.status"}))
                .unwrap()
        );
    }
    assert!(host.validate_snapshot(world, &capture, true).unwrap());
    host.release_snapshot(world, &capture).unwrap();
    host.unload().unwrap();
    host.destroy_world(world).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
}

use super::*;

fn response(raw: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(raw.as_bytes()));
    format!(
        r#"{{"version":1,"ok":true,"data":{{"stamp":{{"world":18446744073709551615,"world_generation":4294967295,"session":"public-identity","frame":9007199254740993,"fixed_tick":2,"schedule_revision":3,"content_revision":4,"snapshot_id":5,"snapshot_sha256":"{digest}"}},"snapshot":{raw}}}}}"#
    )
}

#[test]
fn native_bytes_and_exact_projection_are_distinct_contracts() {
    let raw = r#"{"position":[0.36050745844841003,-0.0,5e-324],"identity":18446744073709551615}"#;
    let text = response(raw);
    let capture = CapturedSnapshot::parse(&text).unwrap();
    assert_eq!(capture.raw_snapshot(), raw);
    assert_eq!(capture.stamp().world(), u64::MAX);
    assert_eq!(capture.stamp().world_generation(), u32::MAX);
    assert_eq!(capture.stamp().frame(), 9_007_199_254_740_993);
    assert_eq!(capture.value()["identity"].as_u64(), Some(u64::MAX));
    let legacy: Value = serde_json::from_str(raw).unwrap();
    assert_ne!(
        legacy["position"][0].as_f64().unwrap().to_bits(),
        capture.value()["position"][0].as_f64().unwrap().to_bits()
    );
    for (index, token) in ["0.36050745844841003", "-0.0", "5e-324"].iter().enumerate() {
        assert_eq!(
            capture.value()["position"][index]
                .as_f64()
                .unwrap()
                .to_bits(),
            token.parse::<f64>().unwrap().to_bits()
        );
    }
    let changed = text.replace("0.36050745844841003", "0.36050745844841004");
    assert!(
        CapturedSnapshot::parse(&changed)
            .err()
            .unwrap()
            .to_string()
            .contains("digest mismatch")
    );
    assert_eq!(capture.raw_snapshot(), raw);
}

#[test]
fn reject_malformed_envelopes_stamps_and_nested_payloads() {
    let good = response(r#"{"x":0}"#);
    for bad in [
        good.replace(r#""version":1"#, r#""version":2"#),
        good.replace(r#""ok":true"#, r#""ok":false"#),
        good.replace(r#""version":1"#, r#""version":1,"version":1"#),
        good.replace(r#""ok":true"#, r#""ok":true,"extra":0"#),
        good.replace(
            r#""frame":9007199254740993"#,
            r#""frame":9007199254740993.0"#,
        ),
        good.replace(r#""fixed_tick":2"#, r#""fixed_tick":2,"fixed_tick":2"#),
        good.replace(
            r#""world_generation":4294967295"#,
            r#""world_generation":4294967296"#,
        ),
        good.replace(
            r#""session":"public-identity""#,
            r#""session":"public-identity","host_key":"forbidden""#,
        ),
        response(r#"{"deep":{"x":1,"\u0078":2}}"#),
        response(r#"{"n":1e999}"#),
        response(&format!(
            "{{\"deep\":{}0{}}}",
            "[".repeat(66),
            "]".repeat(66)
        )),
    ] {
        assert!(CapturedSnapshot::parse(&bad).is_err(), "accepted {bad}");
    }
}

fn snapshot_host() -> (CompatibilityHost, u64) {
    let config = crate::test_support::artifacts().config();
    let mut host = CompatibilityHost::boot(&config).unwrap();
    let world = host.create_world().unwrap();
    host.open_phase_session(world).unwrap();
    let probe = crate::test_support::artifacts().probe();
    host.load_fixture(&json!({"world":world,"assemblies":[probe],"handles":{},
        "requirements":{"execution_profile":"native_phase_context_v1"},
        "graph":{"nodes":[],"components":[],"assets":[],"aliases":{}}}))
        .unwrap();
    (host, world)
}
