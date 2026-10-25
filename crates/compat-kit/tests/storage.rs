#[path = "support/root.rs"]
mod root;
use compat_kit::{RunControl, fixture, runner};
use serde_json::json;
use std::fs;

#[test]
fn worker_storage_cap_includes_hidden_directories_and_discards_oversized_outputs() {
    let root = root::temporary();
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    let data = json!({"schema_version":1,"name":"storage","game":"snake","profiles":["test"],"default_profile":"test","evidence_label":"harness-only",
        "worker":{"program":env!("CARGO_BIN_EXE_compat-kit-protocol-test-worker"),"args":["storage"]},
        "limits":{"deadline_ms":2000,"log_bytes":1024,"artifact_bytes":1_048_576,"capture_count":4},
        "assertions":[{"op":"equal","selector":"counter","value":{"count":7}}]});
    fs::write(
        root.path().join("fixtures/storage.json"),
        serde_json::to_vec(&data).unwrap(),
    )
    .unwrap();
    let result = runner::run(
        root.path(),
        &fixture::load(root.path(), "storage").unwrap(),
        None,
        false,
        &RunControl::default(),
    )
    .unwrap();
    assert_eq!(result.status, compat_kit::Status::Error, "{result:?}");
    assert!(
        !root
            .path()
            .join(result.run_dir)
            .join(".git/oversized.bin")
            .exists()
    );
}

#[test]
fn broken_symlink_output_cannot_escape_worktree() {
    let root = root::temporary();
    fs::create_dir_all(root.path().join(".work")).unwrap();
    std::os::unix::fs::symlink(
        root.path().join("missing"),
        root.path().join(".work/output"),
    )
    .unwrap();
    assert!(
        compat_kit::files::work_path(root.path(), std::path::Path::new(".work/output/report"))
            .is_err()
    );
}
