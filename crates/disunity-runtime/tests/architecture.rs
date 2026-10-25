#[path = "architecture/graph.rs"]
mod graph;
use std::path::Path;

#[test]
fn one_managed_api_and_no_request_transport() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let engine = root.join("csharp/Disunity");
    let mut sources = String::new();
    for entry in std::fs::read_dir(engine).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "cs") {
            sources.push_str(&std::fs::read_to_string(path).unwrap());
        }
    }
    assert!(!sources.contains("namespace Disunity;"));
    assert!(sources.contains("namespace UnityEngine;"));
    for forbidden in [
        "ByteTransport",
        "ByteRequestCodec",
        "JsonSerializer",
        "System.Text.Json",
        "Newtonsoft.Json",
        "HasStarted",
        "Time.Frame +=",
        "disunity_world_step",
    ] {
        assert!(
            !sources.contains(forbidden),
            "duplicate scheduling or transport: {forbidden}"
        );
    }
    assert_eq!(sources.matches("class GameObject").count(), 1);
    assert_eq!(sources.matches("class MonoBehaviour").count(), 1);
    graph::assert_runtime_graph(&root);
    for directory in ["UnityCompat", "CompatibilityBridge"] {
        assert!(!root.join("csharp").join(directory).exists());
    }
}

#[test]
fn unity_consumer_has_only_the_engine_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    graph::assert_managed_graph(&root);
}
