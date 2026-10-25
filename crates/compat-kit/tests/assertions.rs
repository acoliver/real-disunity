#[test]
fn a_missing_pointer_in_the_latest_observation_or_event_cannot_be_hidden() {
    let worker: compat_kit::WorkerReport = serde_json::from_value(serde_json::json!({
        "schema_version":1,"evidence_label":"harness-only","captures":[],"diagnostics":[],
        "teardown":{"completed":true,"native_worlds":0,"managed_roots":0,"pending_readbacks":0},
        "observations":[
            {"selector":"score","frame":0,"phase":"observe","value":{"x":7}},
            {"selector":"score","frame":1,"phase":"observe","value":{}},
            {"selector":"event","frame":2,"phase":"contact","value":{"name":"enter"}},
            {"selector":"event","frame":3,"phase":"contact","value":{}}
        ]
    }))
    .unwrap();
    let assertions: Vec<compat_kit::model::Assertion> = serde_json::from_value(serde_json::json!([
        {"op":"equal","selector":"score","pointer":"/x","value":7},
        {"op":"event_order","selector":"event","pointer":"/name","events":["enter"]}
    ]))
    .unwrap();
    let results = compat_kit::evidence::evaluate(&assertions, &worker, &[]);
    assert!(results.iter().all(|result| !result.passed), "{results:?}");
}

use compat_kit::{RunControl, Status, fixture, runner};
use serde_json::json;
use std::fs;

#[test]
fn real_protocol_observations_support_tolerance_changes_events_and_timeline_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    // Runs take the canonical root, as the CLI resolves it; macOS temp paths are symlinked.
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("fixtures")).unwrap();
    let timeline = json!([
        {"frame":0,"phase":"load","action":"load"},
        {"frame":1,"phase":"input","action":"input","keys":["W"],"axes":{"Vertical":1}},
        {"frame":1,"phase":"update","action":"advance","dt":0.02},
        {"frame":2,"phase":"input","action":"input","keys":[],"axes":{"Vertical":0}},
        {"frame":2,"phase":"unload","action":"unload"}
    ]);
    let path = root.join("fixtures/protocol.json");
    let mut data = json!({"schema_version":1,"name":"protocol","game":"snake","profiles":["test"],"default_profile":"test","evidence_label":"harness-only",
    "worker":{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":["sequence"]},"timeline":timeline,
    "assertions":[
        {"op":"tolerance","selector":"position","pointer":"/x","value":2.0,"tolerance":0.01,"frame":2},
        {"op":"changed","selector":"position","pointer":"/x","before":0,"after":2},
        {"op":"event_order","selector":"event","events":["enter","stay","exit"]},
        {"op":"equal","selector":"timeline","value":timeline},
        {"op":"capture","frame":0,"phase":"render","origin":"harness-only"}
    ]});
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    let run = || {
        runner::run(
            root.as_path(),
            &fixture::load(root.as_path(), "protocol").unwrap(),
            None,
            false,
            &RunControl::default(),
        )
        .unwrap()
    };
    let result = run();
    assert_eq!(result.status, Status::Pass, "{result:?}");
    data["assertions"][0]["tolerance"] = json!(0.00001);
    data["assertions"][2]["events"] = json!(["exit", "enter", "stay"]);
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    let result = run();
    assert_eq!(result.status, Status::Fail);
    assert!(!result.assertions[0].passed);
    assert!(!result.assertions[2].passed);
}
