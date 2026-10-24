use compat_kit::storage::{self, Kind};
use serde_json::json;
use std::fs;

#[test]
fn pruning_reclaims_only_idle_owned_directories_and_preserves_sources_and_leases() {
    let root = tempfile::tempdir().unwrap();
    let old_run = storage::allocate(root.path(), Kind::Run).unwrap();
    let old_path = old_run.path.clone();
    fs::write(old_path.join("evidence"), "old result").unwrap();
    drop(old_run);
    let active_run = storage::allocate(root.path(), Kind::Run).unwrap();
    let accepted = storage::allocate(root.path(), Kind::Candidate).unwrap();
    let accepted_path = accepted.path.clone();
    drop(accepted);
    let retired = storage::allocate(root.path(), Kind::Candidate).unwrap();
    let retired_path = retired.path.clone();
    drop(retired);
    let borrowed = storage::allocate(root.path(), Kind::Candidate).unwrap();
    let borrowed_path = borrowed.path.clone();
    drop(borrowed);
    let _source_lease = storage::source_lease(root.path(), &borrowed_path).unwrap();
    let locks = root.path().join(".work/compat/locks");
    fs::create_dir_all(&locks).unwrap();
    fs::write(
        locks.join("game.json"),
        json!({"source_root":accepted_path}).to_string(),
    )
    .unwrap();
    let foreign = root.path().join(".work/compat/runs/foreign");
    fs::create_dir_all(&foreign).unwrap();
    let removed = storage::prune(root.path(), 0).unwrap();
    assert!(removed.contains(&old_path));
    assert!(removed.contains(&retired_path));
    assert!(!old_path.exists());
    assert!(active_run.path.exists());
    assert!(accepted_path.exists());
    assert!(borrowed_path.exists());
    assert!(foreign.exists());
}

#[test]
fn pruning_never_follows_symlinks_or_removes_configured_source() {
    let root = tempfile::tempdir().unwrap();
    let unsafe_tree = storage::allocate(root.path(), Kind::Candidate).unwrap();
    let unsafe_path = unsafe_tree.path.clone();
    let target = root.path().join("outside-harness");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep"), "other agent").unwrap();
    std::os::unix::fs::symlink(&target, unsafe_path.join("link")).unwrap();
    drop(unsafe_tree);
    let source = storage::allocate(root.path(), Kind::Candidate).unwrap();
    let source_path = source.path.clone();
    drop(source);
    fs::create_dir_all(root.path().join("fixtures")).unwrap();
    fs::write(
        root.path().join("fixtures/local.json"),
        json!({
            "schema_version":1,"name":"local","source_root":source_path,"profiles":["test"],
            "default_profile":"test","evidence_label":"harness-only"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        storage::prune(root.path(), 0).unwrap(),
        Vec::<std::path::PathBuf>::new()
    );
    assert_eq!(
        fs::read_to_string(target.join("keep")).unwrap(),
        "other agent"
    );
    assert!(unsafe_path.exists());
    assert!(source_path.exists());
}
