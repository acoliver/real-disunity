use super::{CompatibilityHost, Value, json};

pub fn public_evidence(host: &mut CompatibilityHost, world: u64, authority: &Value) {
    let key = authority["host_key"].as_str().unwrap();
    let mut evidence = Vec::new();
    for op in ["describe", "snapshot", "session.status"] {
        evidence.push(host.request(world, &json!({"op":op})).unwrap().to_string());
    }
    for op in ["managed.snapshot", "managed.statistics", "managed.failures"] {
        evidence.push(host.observe(&json!({"op":op})).unwrap().to_string());
    }
    let error = host.dispatch_phase(&json!({"phase":"Update"})).unwrap_err();
    evidence.push(error.to_string());
    evidence.push(format!("{error:?}"));
    for public in evidence {
        assert!(
            !public.contains(key),
            "public evidence leaked private authority"
        );
        assert!(
            !public.contains("host_key"),
            "public evidence exposed private field"
        );
    }
}
