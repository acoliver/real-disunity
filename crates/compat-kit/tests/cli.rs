use compat_kit::Status;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

/// A temporary workspace addressed by its canonical path, as the CLI resolves its root.
/// macOS temporary directories sit behind the /var symlink.
struct Root {
    _temp: TempDir,
    path: PathBuf,
}
impl Root {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn setup(mode: &str) -> Root {
    let temp = tempfile::tempdir().unwrap();
    let dir = Root {
        path: temp.path().canonicalize().unwrap(),
        _temp: temp,
    };
    fs::create_dir_all(dir.path().join("fixtures")).unwrap();
    let fixture = json!({
        "schema_version": 1, "name": "harness-only", "game": "snake",
        "profiles": ["test"], "default_profile": "test", "evidence_label": "harness-only",
        "worker": {"program": env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"), "args": [mode]},
        "limits": {"deadline_ms": 350, "log_bytes": 1024, "artifact_bytes": 1_048_576, "capture_count": 4},
        "timeline": [{"frame": 0, "phase": "update", "action": "observe", "selector": "counter"}],
        "assertions": [{"op": "equal", "selector": "counter", "pointer": "/count", "value": 7}]
    });
    fs::write(
        dir.path().join("fixtures/harness-only.json"),
        serde_json::to_vec_pretty(&fixture).unwrap(),
    )
    .unwrap();
    dir
}

fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_compat-kit"))
        .env("COMPAT_ROOT", root)
        .args(args)
        .output()
        .unwrap()
}
fn run(root: &Path) -> Value {
    let out = cli(root, &["run", "harness-only"]);
    assert!(
        !out.stdout.is_empty(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn removed_source_refresh_command_is_rejected() {
    let root = setup("pass");
    let output = cli(root.path(), &["fixture", "refresh", "harness-only"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.path().join(".work").exists());
}

#[test]
fn source_resolution_failure_is_blocked_before_starting_a_worker() {
    let root = setup("pass");
    let mut fixture = compat_kit::fixture::load(root.path(), "harness-only").unwrap();
    fixture.game = "unknown".into();
    let record = compat_kit::runner::run(
        root.path(),
        &fixture,
        None,
        false,
        &compat_kit::RunControl::default(),
    )
    .unwrap();
    assert_eq!(record.status, Status::Blocked);
    assert!(record.worker.is_none());
    assert!(
        record
            .messages
            .iter()
            .any(|m| m.contains("unknown original game"))
    );
    assert!(
        !root
            .path()
            .join(record.run_dir)
            .join("request.json")
            .exists()
    );
}

#[test]
fn harness_only_real_worker_evidence_passes() {
    let root = setup("pass");
    let result = run(root.path());
    assert_eq!(result["status"], "pass");
    assert_eq!(result["evidence_label"], "harness-only");
    assert_eq!(result["assertions"][0]["passed"], true);
    assert_eq!(result["captures"][0]["width"], 2);
    assert!(
        root.path()
            .join(result["run_dir"].as_str().unwrap())
            .join("run.json")
            .is_file()
    );
}

#[test]
fn teardown_failure_preserves_validated_partial_captures_and_assertions() {
    let root = setup("leak");
    let result = run(root.path());
    assert_eq!(result["status"], "error");
    assert!(result["messages"].to_string().contains("teardown"));
    assert_eq!(
        result["captures"][0]["width"], 2,
        "measured teardown failure must retain actual validated evidence"
    );
    assert_eq!(result["assertions"][0]["passed"], true);
}

#[test]
fn harness_only_worker_failures_have_distinct_outcomes() {
    for (mode, status, message) in [
        ("mismatch", "fail", "assertion"),
        ("crash", "error", "exit"),
        ("hang", "error", "deadline"),
        ("malformed", "error", "response"),
        ("traversal", "error", "capture"),
        ("missing-image", "error", "capture"),
        ("bad-image", "error", "PNG"),
        ("leak", "error", "teardown"),
    ] {
        let root = setup(mode);
        let result = run(root.path());
        assert_eq!(result["status"], status, "{mode}: {result}");
        assert!(
            result["messages"].to_string().contains(message),
            "{mode}: {result}"
        );
    }
}

#[test]
fn absence_of_assertions_and_missing_prerequisites_never_pass() {
    let root = setup("pass");
    let path = root.path().join("fixtures/harness-only.json");
    let mut fixture: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    fixture["assertions"] = json!([]);
    fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    assert_eq!(run(root.path())["status"], "inconclusive");
    fixture["worker"]["program"] = json!("/missing/compat-worker");
    fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    assert_eq!(run(root.path())["status"], "blocked");
    let out = cli(
        root.path(),
        &["run", "harness-only", "--profile", "unknown"],
    );
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["status"], "unsupported");
}

#[test]
fn bounded_logs_and_actual_image_dashboard_with_no_record_cells() {
    let root = setup("log-flood");
    let result = run(root.path());
    assert_eq!(result["status"], "pass", "{result}");
    let run_dir = root.path().join(result["run_dir"].as_str().unwrap());
    assert!(fs::metadata(run_dir.join("stdout.log")).unwrap().len() <= 1024);
    let out = cli(root.path(), &["dashboard"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let page = fs::read_to_string(root.path().join(".work/compat/dashboard/index.html")).unwrap();
    assert!(page.contains("harness-only"));
    assert!(page.contains("<img"));
    assert!(page.contains("Input timeline"));
    assert!(page.contains("Unity 6.3"));
    assert!(page.contains("status-filter"));
    assert!(page.contains("not-implemented"));
    assert!(page.contains("observed"));
    fs::remove_file(run_dir.join("captures/frame.png")).unwrap();
    cli(root.path(), &["dashboard"]);
    let page = fs::read_to_string(root.path().join(".work/compat/dashboard/index.html")).unwrap();
    assert!(page.contains("missing capture"));
    fs::write(run_dir.join("request.json"), "{truncated").unwrap();
    let out = cli(root.path(), &["dashboard"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let page = fs::read_to_string(root.path().join(".work/compat/dashboard/index.html")).unwrap();
    assert!(page.contains("damaged request"));
}

#[test]
fn pinned_checkout_is_sent_without_source_hashes_and_report_paths_are_checked() {
    let root = setup("pass");
    let result = run(root.path());
    assert_eq!(result["status"], "pass");
    let directory = root.path().join(result["run_dir"].as_str().unwrap());
    let request: Value =
        serde_json::from_slice(&fs::read(directory.join("request.json")).unwrap()).unwrap();
    let source =
        worldrunner_original_games::checkout(worldrunner_original_games::Game::Snake).unwrap();
    assert_eq!(request["source_root"], source.to_str().unwrap());
    assert!(request.get("source_hashes").is_none());
    assert!(result.get("source_before").is_none());
    assert!(result["reporting"].get("identities").is_none());
    let mut forged = result;
    forged["run_id"] = json!("../other-result");
    fs::write(
        directory.join("run.json"),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    let (records, errors) = compat_kit::dashboard::records(root.path()).unwrap();
    assert!(records.is_empty());
    assert!(
        errors.iter().any(|e| e.contains("report identity")),
        "{errors:?}"
    );
}

#[test]
fn catalog_display_name_runs_the_same_fixture_as_its_filename() {
    let root = setup("pass");
    let path = root.path().join("fixtures/harness-only.json");
    let mut fixture: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    fixture["name"] = json!("JourneyDisplayName");
    fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let out = cli(root.path(), &["run", "JourneyDisplayName"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let record: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(record["status"], "pass");
    assert_eq!(record["fixture"], "JourneyDisplayName");
    assert!(
        cli(root.path(), &["fixture", "check", "harness-only"])
            .status
            .success()
    );
}

#[test]
fn duplicate_catalog_display_names_are_rejected_instead_of_selecting_a_worker() {
    let root = setup("pass");
    let path = root.path().join("fixtures/harness-only.json");
    fs::copy(path, root.path().join("fixtures/second.json")).unwrap();
    assert!(
        compat_kit::fixture::catalog(root.path())
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(
        compat_kit::fixture::load(root.path(), "missing-name")
            .unwrap_err()
            .contains("duplicate")
    );
}

#[test]
fn nine_status_worker_protocol_preserves_outcomes_and_failure_dominance() {
    let statuses = [
        ("pass", "pass"),
        ("fail", "fail"),
        ("unsupported", "unsupported"),
        ("not-implemented", "not-implemented"),
        ("blocked", "blocked"),
        ("skipped", "skipped"),
        ("error", "error"),
        ("flaky", "flaky"),
        ("inconclusive", "inconclusive"),
    ];
    for (wire, canonical) in statuses {
        let parsed: Status = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), json!(canonical));
        let root = setup(&format!("diagnostic-{wire}"));
        let fixture_path = root.path().join("fixtures/harness-only.json");
        let mut fixture: Value = serde_json::from_slice(&fs::read(&fixture_path).unwrap()).unwrap();
        fixture["worker"]["args"] = json!([format!("diagnostic-{wire}")]);
        fs::write(&fixture_path, serde_json::to_vec(&fixture).unwrap()).unwrap();
        let result = run(root.path());
        assert_eq!(result["status"], canonical, "diagnostic {wire}: {result}");
    }
    assert!(serde_json::from_str::<Status>("\"not_implemented\"").is_err());
}

#[test]
fn measured_assertion_failure_cannot_be_overridden_by_worker_pass() {
    let root = setup("diagnostic-pass");
    let fixture_path = root.path().join("fixtures/harness-only.json");
    let mut fixture: Value = serde_json::from_slice(&fs::read(&fixture_path).unwrap()).unwrap();
    fixture["worker"]["args"] = json!(["diagnostic-pass"]);
    fixture["assertions"][0]["value"] = json!(99);
    fs::write(fixture_path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    assert_eq!(run(root.path())["status"], "fail");
}
