#[path = "support/root.rs"]
mod root;
use compat_kit::storage;
use std::fs;

#[test]
fn pruning_reclaims_only_idle_owned_runs_and_preserves_live_leases() {
    let root = root::temporary();
    let old_run = storage::allocate(root.path()).unwrap();
    let old_path = old_run.path.clone();
    fs::write(old_path.join("result"), "old result").unwrap();
    drop(old_run);
    let active_run = storage::allocate(root.path()).unwrap();
    let foreign = root.path().join(".work/compat/runs/foreign");
    fs::create_dir_all(&foreign).unwrap();
    let removed = storage::prune(root.path(), 0).unwrap();
    assert_eq!(removed, vec![old_path.clone()]);
    assert!(!old_path.exists());
    assert!(active_run.path.exists());
    assert!(foreign.exists());
    let active_path = active_run.path.clone();
    drop(active_run);
    assert_eq!(storage::prune(root.path(), 0).unwrap(), vec![active_path]);
}

#[test]
fn pruning_never_follows_symlinks_or_removes_original_game_storage() {
    let root = root::temporary();
    let unsafe_tree = storage::allocate(root.path()).unwrap();
    let unsafe_path = unsafe_tree.path.clone();
    let target = root.path().join("outside-harness");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep"), "other agent").unwrap();
    std::os::unix::fs::symlink(&target, unsafe_path.join("link")).unwrap();
    drop(unsafe_tree);
    let source_path = root.path().join("target/original-games");
    fs::create_dir_all(&source_path).unwrap();
    fs::write(source_path.join("keep"), "original input").unwrap();
    assert!(storage::prune(root.path(), 0).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(target.join("keep")).unwrap(),
        "other agent"
    );
    assert!(unsafe_path.exists());
    assert!(source_path.exists());
}
