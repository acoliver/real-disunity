use compat_kit::RunRequest;
use compat_player::{Installation, run_request};
use serde_json::json;

#[path = "../src/test_support.rs"]
pub mod support;

#[test]
fn unchanged_snake_scene_binds_and_updates_native_objects_then_reloads() {
    let installation = support::installation();
    let artifacts = support::scratch("snake-live-");
    let request = request(&installation, artifacts.path());
    let report = run_request(&installation, &request);
    assert!(
        report.diagnostics.is_empty(),
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    assert_eq!(report.observations.len(), 3);
    assert!(
        report.observations[1].value["objects"]
            .as_array()
            .unwrap()
            .len()
            > report.observations[0].value["objects"]
                .as_array()
                .unwrap()
                .len(),
        "unchanged SnakeView must create its real native segment clone after Update"
    );
    assert_ne!(
        report.observations[0].value["objects"][0]["handle"],
        report.observations[2].value["objects"][0]["handle"],
        "reload must retire native generations"
    );
    assert_eq!(
        report.captures.len(),
        1,
        "capture must come from the canonical GPU readback"
    );
    assert!(artifacts.path().join("captures/snake.png").is_file());
    assert!(report.teardown.completed);
    assert_eq!(report.teardown.native_worlds, 0);
    assert_eq!(report.teardown.managed_roots, 0);
    sequential_sessions(&installation, &request, artifacts);
    failed_binding_is_observed_after_cleanup(&installation, request);
}

fn sequential_sessions(
    installation: &Installation,
    previous: &RunRequest,
    artifacts: tempfile::TempDir,
) {
    sequential_session_uses_the_same_process_image(installation, previous, artifacts.path());
    drop(artifacts);
    let distinct = support::scratch("snake-sequential-");
    sequential_session_uses_the_same_process_image(installation, previous, distinct.path());
}

fn sequential_session_uses_the_same_process_image(
    installation: &Installation,
    previous: &RunRequest,
    artifacts: &std::path::Path,
) {
    let mut request = previous.clone();
    request.artifact_root = artifacts.into();
    request.timeline = vec![
        json!({"frame":0,"phase":"load","action":"load"}),
        json!({"frame":1,"phase":"update","action":"advance","dt":0.02}),
        json!({"frame":1,"phase":"observe","action":"observe","selector":"world.snapshot"}),
        json!({"frame":2,"phase":"unload","action":"unload"}),
    ];
    let report = run_request(installation, &request);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.observations.len(), 1);
    assert!(report.teardown.completed);
    assert_eq!(report.teardown.native_worlds, 0);
    assert_eq!(report.teardown.managed_roots, 0);
}

fn request(installation: &Installation, artifacts: &std::path::Path) -> RunRequest {
    let root = installation.root();
    RunRequest {
        schema_version: 1,
        run_id: "snake-integration".into(),
        fixture: "snake".into(),
        profile: "compat-6000.3".into(),
        desktop: false,
        seed: 17,
        source_root: worldrunner_original_games::checkout(worldrunner_original_games::Game::Snake)
            .unwrap(),
        artifact_root: artifacts.into(),
        assertions: vec![],
        metadata: json!({"scene":"Assets/Scenes/GameScene.unity","script_count":8,
            "import_profile":root.join("crates/compat-content/profiles/snake-source.json"),
            "compatibility_sidecar":root.join("crates/compat-content/overlays/snake.json"),
            "admission_policy":root.join("crates/compat-content/profiles/snake-admission.json"),
            "evidence_label":"original-source/authored-missing-sprite"}),
        timeline: vec![
            json!({"frame":0,"phase":"load","action":"load"}),
            json!({"frame":0,"phase":"observe","action":"observe","selector":"world.snapshot"}),
            json!({"frame":1,"phase":"input","action":"input","keys":["W"],"axes":{}}),
            json!({"frame":1,"phase":"update","action":"advance","dt":0.2}),
            json!({"frame":1,"phase":"observe","action":"observe","selector":"world.snapshot"}),
            json!({"frame":1,"phase":"render","action":"capture","path":"captures/snake.png"}),
            json!({"frame":2,"phase":"reload","action":"reload"}),
            json!({"frame":2,"phase":"observe","action":"observe","selector":"world.snapshot"}),
        ],
    }
}

fn failed_binding_is_observed_after_cleanup(installation: &Installation, mut request: RunRequest) {
    let artifacts = support::scratch("snake-binding-");
    request.artifact_root = artifacts.path().into();
    request.metadata["managed_profile"] =
        json!({"field_aliases":{"GameLoop.config":"missing_config_field"}});
    request.timeline = vec![json!({"frame":0,"phase":"load","action":"load"})];
    let report = run_request(installation, &request);
    assert!(
        report.diagnostics.iter().any(|entry| entry
            .message
            .contains("unknown serialized field 'GameLoop.config'")),
        "{:?}",
        report.diagnostics
    );
    assert!(
        report.teardown.completed,
        "failed partial load must remain available for measured cleanup"
    );
    assert_eq!(report.teardown.native_worlds, 0);
    assert_eq!(report.teardown.managed_roots, 0);
    assert!(report.captures.is_empty());
    failed_public_open_releases_partial_world(installation, request);
}

fn failed_public_open_releases_partial_world(installation: &Installation, mut request: RunRequest) {
    assert!(compat_player::Session::open(installation, &request).is_err());
    request.metadata["managed_profile"] = json!({});
    let mut session = compat_player::Session::open(installation, &request).unwrap();
    assert_eq!(
        session.statistics().unwrap().0,
        1,
        "failed open must release its native world"
    );
    session.unload().unwrap();
    assert_eq!(session.statistics().unwrap().0, 0);
    assert_eq!(session.managed_roots().unwrap(), 0);
}
