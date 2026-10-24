use compat_kit::{dashboard, fixture};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn setup() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("fixtures")).unwrap();
    let fixture = json!({
        "schema_version":1,"name":"Discovery","source_root":".work/source",
        "profiles":["test"],"default_profile":"test","evidence_label":"harness-only",
        "worker":{"program":"/not-executed","args":[]},
        "limits":{"deadline_ms":350,"log_bytes":1024,"artifact_bytes":1_048_576,"capture_count":4},
        "timeline":[],"assertions":[]
    });
    write(root.path(), "fixture.json", &fixture);
    root
}

fn write(root: &Path, name: &str, data: &Value) {
    fs::write(
        root.join("fixtures").join(name),
        serde_json::to_vec(data).unwrap(),
    )
    .unwrap();
}

fn traceability() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/compatibility-catalog.json")).unwrap()
}

#[test]
fn production_traceability_manifest_does_not_prevent_fixture_discovery() {
    let root = setup();
    write(root.path(), "compatibility-catalog.json", &traceability());
    let fixtures = fixture::catalog(root.path()).unwrap();
    assert_eq!(fixtures.len(), 1);
    assert_eq!(fixtures[0].name, "Discovery");
    let state = dashboard::state(root.path()).unwrap();
    assert_eq!(state["fixtures"].as_array().unwrap().len(), 1);
}

#[test]
fn manifest_schema_at_an_unreserved_path_is_not_silently_ignored() {
    let root = setup();
    write(root.path(), "traceability-copy.json", &traceability());
    assert!(fixture::catalog(root.path()).is_err());
}

#[test]
fn reserved_manifest_requires_its_exact_schema_and_document_shape() {
    for change in ["schema", "cases", "worker"] {
        let root = setup();
        let mut manifest = traceability();
        manifest[change] = match change {
            "schema" => json!("compatibility-traceability/unknown"),
            "cases" => json!({"unexpected":"object"}),
            _ => json!({"program":"/not-executed"}),
        };
        write(root.path(), "compatibility-catalog.json", &manifest);
        assert!(fixture::catalog(root.path()).is_err(), "{change}");
    }
}

#[test]
fn malformed_executable_fixtures_still_fail_discovery() {
    let root = setup();
    let mut fixture: Value =
        serde_json::from_slice(&fs::read(root.path().join("fixtures/fixture.json")).unwrap())
            .unwrap();
    fixture["unexpected"] = json!(true);
    write(root.path(), "fixture.json", &fixture);
    write(root.path(), "compatibility-catalog.json", &traceability());
    assert!(fixture::catalog(root.path()).is_err());
}
