#[path = "support/root.rs"]
mod root;
use compat_kit::{Fixture, fixture};
use serde_json::json;
use std::{fs, path::Path};

fn specification() -> serde_json::Value {
    json!({"schema_version":1,"name":"probe","game":"snake",
        "profiles":["test"],"default_profile":"test","evidence_label":"harness-only"})
}

#[test]
fn discovery_requires_executable_fixtures_for_every_json_file() {
    let root = root::temporary();
    let fixtures = root.path().join("fixtures");
    fs::create_dir(&fixtures).unwrap();
    let path = fixtures.join("anything.json");
    fs::write(&path, json!({"schema":"unrelated-document"}).to_string()).unwrap();
    assert!(fixture::catalog(root.path()).is_err());
    fs::write(&path, specification().to_string()).unwrap();
    assert_eq!(fixture::catalog(root.path()).unwrap()[0].name, "probe");
}

#[test]
fn fixture_sources_are_named_games_and_removed_configuration_is_rejected() {
    let root = root::temporary();
    let path = root.path().join("probe.json");
    fs::write(&path, specification().to_string()).unwrap();
    fixture::load(root.path(), path.to_str().unwrap()).unwrap();
    for (key, value) in [
        ("source_root", json!("arbitrary-source")),
        ("source_hashes", json!({})),
        (
            "refresh",
            json!({"remote":"origin","branch":"main","gates":[]}),
        ),
    ] {
        let mut spec = specification();
        spec[key] = value;
        assert!(serde_json::from_value::<Fixture>(spec).is_err());
    }
    let mut spec = specification();
    spec["game"] = json!("unknown");
    fs::write(&path, spec.to_string()).unwrap();
    assert!(fixture::load(root.path(), path.to_str().unwrap()).is_err());
}

#[test]
fn shipped_fixtures_use_pinned_games_and_repository_owned_fonts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let fixtures = fixture::catalog(root).unwrap();
    assert!(!fixtures.is_empty());
    for fixture in fixtures {
        let value = serde_json::to_value(&fixture).unwrap();
        value["game"]
            .as_str()
            .unwrap()
            .parse::<worldrunner_original_games::Game>()
            .unwrap();
        let program = &fixture.worker.unwrap().program;
        assert_eq!(program, "{root}/target/debug/compat-player");
        assert!(!value.to_string().contains(".work/"));
        if let Some(font) = value["metadata"]["fonts"]["host.builtin-font"].as_str() {
            assert!(root.join(font).is_file());
            assert!(
                root.join(value["metadata"]["font_license"].as_str().unwrap())
                    .is_file()
            );
        }
    }
}
