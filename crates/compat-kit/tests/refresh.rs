use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn commit(root: &Path, text: &str) -> String {
    fs::write(root.join("Assets/Game.cs"), text).unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Harness",
            "-c",
            "user.email=harness@localhost",
            "commit",
            "-m",
            "fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
#[test]
fn local_git_refresh_qualifies_and_atomically_adopts_without_rewriting_original() {
    exercise("gate-pass", true);
}
#[test]
fn candidate_qualification_can_run_the_normal_harness_under_its_source_lease() {
    exercise("gate-run-candidate", true);
}
#[test]
fn local_git_failed_gate_preserves_active_source_and_lock() {
    exercise("gate-fail", false);
}
#[test]
fn candidate_modified_by_gate_never_adopted() {
    exercise("gate-mutate", false);
}
#[test]
fn post_gate_local_ref_changes_never_adopt() {
    exercise("gate-ref-drift", false);
}
#[test]
fn post_gate_fetch_identity_changes_never_adopt() {
    exercise("gate-fetch-drift", false);
}
#[test]
fn local_upstream_branch_movement_after_runtime_never_adopts() {
    exercise("gate-upstream-ref-drift", false);
}
#[test]
fn explicitly_requested_missing_closure_proof_never_adopts() {
    exercise_selected(
        "gate-pass",
        false,
        Some(json!({"schema_version":1,"mode":"strict-closure"})),
        "fail",
    );
}
#[test]
fn dependency_admission_cannot_be_enabled_without_trusted_host_integration() {
    exercise_selected(
        "gate-pass",
        false,
        Some(json!({"schema_version":1,"mode":"dependency-admission"})),
        "blocked",
    );
}
#[test]
fn fixture_cannot_select_an_arbitrary_integrity_checker() {
    exercise_selected(
        "gate-pass",
        false,
        Some(json!({"schema_version":1,"mode":"strict-closure","checker":"arbitrary"})),
        "blocked",
    );
}
fn exercise(gate: &str, success: bool) {
    exercise_selected(gate, success, None, if success { "pass" } else { "fail" });
}
fn exercise_selected(gate: &str, success: bool, integrity: Option<Value>, expected_status: &str) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join(".work/upstream");
    fs::create_dir_all(upstream.join("Assets")).unwrap();
    git(&upstream, &["init", "-b", "main"]);
    let old = commit(&upstream, "class Original {}\n");
    let source = root.path().join(".work/source");
    let out = Command::new("git")
        .args(["clone", "--no-hardlinks"])
        .arg(&upstream)
        .arg(&source)
        .output()
        .unwrap();
    assert!(out.status.success());
    let new = commit(&upstream, "class Updated { public void AddedApi() {} }\n");
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    let mut fixture = json!({"schema_version":1,"name":"local","source_root":".work/source","profiles":["test"],"default_profile":"test","evidence_label":"harness-only",
        "refresh":{"remote":"origin","branch":"main","gates":[{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":[gate]}]}});
    if let Some(selection) = integrity {
        fixture["metadata"] = json!({"refresh_integrity":selection});
    }
    fs::write(
        root.path().join("fixtures/local.json"),
        serde_json::to_vec(&fixture).unwrap(),
    )
    .unwrap();
    let out = refresh_cli(root.path());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], expected_status, "{report}");
    assert_eq!(git(&source, &["rev-parse", "HEAD"]), old);
    let lock = root.path().join(".work/compat/locks/local.json");
    if success {
        let lock: Value = serde_json::from_slice(&fs::read(&lock).unwrap()).unwrap();
        assert_eq!(lock["revision"], new);
        assert_eq!(lock["integrity"]["engine_proof"], false);
        assert_eq!(lock["integrity"]["scope"], "harness-gates-only");
        let adopted = Path::new(lock["source_root"].as_str().unwrap());
        assert_eq!(git(adopted, &["rev-parse", "HEAD"]), new);
        assert!(
            fs::read_to_string(adopted.join("Assets/Game.cs"))
                .unwrap()
                .contains("AddedApi")
        );
        assert!(
            report["diff"]["api_lines"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str().unwrap().contains("AddedApi"))
        );
    } else {
        assert!(!lock.exists());
    }
    if success {
        rejects_failed_replacement(root.path(), &lock);
        rejects_concurrent_original_commit(root.path(), &lock);
    }
    rejects_dirty_original(root.path(), &source);
}

fn rejects_dirty_original(root: &Path, source: &Path) {
    // Dirty originals are preserved and cannot be used to qualify an update.
    fs::write(source.join("Assets/Game.cs"), "local edit").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_compat-kit"))
        .env("COMPAT_ROOT", root)
        .args(["fixture", "refresh", "local", "--accept"])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["status"], "blocked", "{report}");
    assert_eq!(
        fs::read_to_string(source.join("Assets/Game.cs")).unwrap(),
        "local edit"
    );
}

fn refresh_cli(root: &Path) -> std::process::Output {
    let out = Command::new(env!("CARGO_BIN_EXE_compat-kit"))
        .env("COMPAT_ROOT", root)
        .args(["fixture", "refresh", "local", "--accept"])
        .output()
        .unwrap();
    assert!(
        !out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn rejects_failed_replacement(root: &Path, lock: &Path) {
    let previous = fs::read(lock).unwrap();
    let config = root.join("fixtures/local.json");
    let mut fixture: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    fixture["refresh"]["gates"][0]["args"] = json!(["gate-fail"]);
    fs::write(config, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let output = refresh_cli(root);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(
        fs::read(lock).unwrap(),
        previous,
        "failed replacement changed active-source lock"
    );
}

fn rejects_concurrent_original_commit(root: &Path, lock: &Path) {
    let previous = fs::read(lock).unwrap();
    let config = root.join("fixtures/local.json");
    let mut fixture: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    fixture["refresh"]["gates"][0]["args"] = json!(["gate-original-commit"]);
    fs::write(config, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let output = refresh_cli(root);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "fail", "{report}");
    assert_eq!(fs::read(lock).unwrap(), previous);
}
