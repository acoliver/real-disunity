use serde_json::{Value, json};
use worldrunner_disunity_runtime::CompatibilityHost;

use super::{control, manual_unload, prepare_contacts};

pub fn controls(host: &mut CompatibilityHost, raw: &super::raw::RawBridge) {
    for case in [
        "raw",
        "equivalent",
        "geometry",
        "signed-zero",
        "identity",
        "large-identity",
        "null",
        "duplicate",
        "order",
        "duplicate-property",
        "missing",
        "extra",
        "conflicting",
        "invalid-response",
    ] {
        check(host, raw, case);
    }
}

fn envelope(events: &Value) -> Value {
    json!({"version":1,"ok":true,"data":events})
}

fn supplied(case: &str, context: &Value, events: &Value) -> Value {
    let mut response = envelope(events);
    match case {
        "geometry" => response["data"][0]["point"][0] = json!(f64::from_bits(1)),
        "signed-zero" => {
            response["data"][0]["point"][0] = json!(f64::from_bits(
                events[0]["point"][0].as_f64().unwrap().to_bits() ^ (1_u64 << 63)
            ))
        }
        "identity" => response["data"][0]["a"] = json!(events[0]["a"].as_u64().unwrap() + 1),
        "large-identity" => response["data"][0]["a"] = json!(9_007_199_254_740_993_u64),
        "null" => response["data"][0]["body_a"] = json!(0),
        "duplicate" => response["data"]
            .as_array_mut()
            .unwrap()
            .push(events[0].clone()),
        "order" => response["data"].as_array_mut().unwrap().reverse(),
        "missing" => {
            response["data"][0].as_object_mut().unwrap().remove("a");
        }
        "extra" => response["data"][0]["injected"] = json!(true),
        "invalid-response" => response["ok"] = json!(false),
        _ => {}
    }
    let mut text = response.to_string();
    if case == "equivalent" {
        text = text.replace("0.0", "0e0").replace("1.0", "1e0");
    }
    if case == "duplicate-property" {
        text = text.replace("\"trigger\":", "\"trigger\":false,\"trigger\":");
    }
    let mut request = json!({"context":context,"events_response":text});
    if case == "conflicting" {
        request["events"] = json!([]);
    }
    request
}

fn check(host: &mut CompatibilityHost, raw: &super::raw::RawBridge, case: &str) {
    let world = host.create_world().unwrap();
    let (authority, peer, context) = prepare_contacts(host, world, true);
    let events = host
        .request(world, &json!({"op":"events.read","clear":false}))
        .unwrap();
    let request = supplied(case, &context, &events);
    if case == "raw" {
        reject_unclaimed(host, raw, world, &request, &events);
    }
    let accepted = matches!(case, "raw" | "equivalent");
    let error = host.invoke_contact(&request).unwrap_err().to_string();
    assert!(
        error.contains(if accepted {
            "solver contact sentinel"
        } else {
            "strict contacts"
        }),
        "{case}: {error}"
    );
    let remaining = host
        .request(world, &json!({"op":"events.read","clear":false}))
        .unwrap();
    assert_eq!(
        remaining,
        if accepted { json!([]) } else { events },
        "{case}"
    );
    let position = host
        .request(world, &json!({"op":"transform.get","object":peer}))
        .unwrap();
    assert_eq!(
        position["position"][2],
        if accepted { 2. } else { 0. },
        "{case}"
    );
    assert!(
        host.invoke_contact(&request).is_err(),
        "duplicate claim: {case}"
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

fn reject_unclaimed(
    host: &mut CompatibilityHost,
    raw: &super::raw::RawBridge,
    world: u64,
    request: &Value,
    events: &Value,
) {
    let before = host.request(world, &json!({"op":"snapshot"})).unwrap();
    assert_ne!(
        raw.contact_on_thread(request),
        0,
        "foreign thread consumed Contacts"
    );
    for bad_context in [Value::Null, json!({"phase":"Contacts","token":"injected"})] {
        let mut bad = request.clone();
        bad["context"] = bad_context;
        assert!(host.invoke_contact(&bad).is_err());
        assert_eq!(
            host.request(world, &json!({"op":"snapshot"})).unwrap(),
            before
        );
        assert_eq!(
            host.request(world, &json!({"op":"events.read","clear":false}))
                .unwrap(),
            *events
        );
    }
}
