use super::*;
use std::os::unix::fs::symlink;

#[test]
fn live_scan_restarts_after_real_entry_disappears_before_metadata() {
    let root = tempfile::tempdir().unwrap();
    let removed = root.path().join("temporary");
    fs::write(&removed, "x").unwrap();
    let mut first = true;
    let paths = scanner(root.path(), 10, 10, false, |path| {
        if first {
            first = false;
            fs::remove_file(path).unwrap();
            fs::write(root.path().join(".retained"), "data").unwrap();
        }
    })
    .live()
    .unwrap();
    assert_eq!(paths, vec![root.path().join(".retained")]);
}

#[test]
fn strict_scan_rejects_real_disappearance_with_path_and_operation() {
    let root = tempfile::tempdir().unwrap();
    let removed = root.path().join("temporary");
    fs::write(&removed, "x").unwrap();
    let error = scanner(root.path(), 10, 10, false, |path| {
        fs::remove_file(path).unwrap();
    })
    .once()
    .unwrap_err();
    assert!(
        matches!(error, ScanError::Io { ref path, operation: "metadata", ref error, .. } if path == &removed && error.kind() == io::ErrorKind::NotFound)
    );
}

#[test]
fn continuously_disappearing_entries_exhaust_exactly_three_scans() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("temporary");
    fs::write(&target, "x").unwrap();
    let mut attempts = 0;
    let error = scanner(root.path(), 10, 10, false, |path| {
        attempts += 1;
        fs::remove_file(path).unwrap();
        fs::write(root.path().join(format!("next-{attempts}")), "x").unwrap();
    })
    .live()
    .unwrap_err();
    assert_eq!(attempts, 3);
    assert!(matches!(error, ScanError::Exhausted { attempts: 3, .. }));
    assert!(error.to_string().contains("metadata"));
}

#[test]
fn retry_discards_partial_counts_and_checks_hidden_files_and_caps() {
    for (bytes, files) in [(3, 10), (10, 0)] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("temporary"), "x").unwrap();
        let mut first = true;
        let error = scanner(root.path(), bytes, files, false, |path| {
            if first {
                first = false;
                fs::remove_file(path).unwrap();
                fs::write(root.path().join(".over-cap"), "data").unwrap();
            }
        })
        .live()
        .unwrap_err();
        assert!(matches!(
            error,
            ScanError::Rejected {
                reason: "artifact/source storage cap exceeded",
                ..
            }
        ));
    }
}

#[test]
fn retry_never_accepts_a_symlink_replacement() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("temporary"), "x").unwrap();
    let mut first = true;
    let error = scanner(root.path(), 10, 10, false, |path| {
        if first {
            first = false;
            fs::remove_file(path).unwrap();
            symlink("/etc/passwd", root.path().join("unsafe")).unwrap();
        }
    })
    .live()
    .unwrap_err();
    assert!(matches!(
        error,
        ScanError::Rejected {
            reason: "symlink rejected",
            ..
        }
    ));
}

#[test]
fn root_disappearance_or_symlink_is_always_fatal() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("scan");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("temporary"), "x").unwrap();
    let error = scanner(&directory, 10, 10, false, |path| {
        fs::remove_file(path).unwrap();
        fs::remove_dir(&directory).unwrap();
    })
    .live()
    .unwrap_err();
    assert!(matches!(
        error,
        ScanError::Io {
            descendant: false,
            ..
        }
    ));
    symlink(root.path(), &directory).unwrap();
    assert!(live_tree_files(&directory, 10, 10).is_err());
    assert!(scan(&directory, 10, 10, false).is_err());
}

#[test]
fn special_files_and_budget_fail_without_a_retry() {
    let root = tempfile::tempdir().unwrap();
    let socket = std::os::unix::net::UnixListener::bind(root.path().join("socket")).unwrap();
    let mut calls = 0;
    let error = scanner(root.path(), 10, 10, false, |_| {
        calls += 1;
    })
    .live()
    .unwrap_err();
    assert!(matches!(
        error,
        ScanError::Rejected {
            reason: "nonregular artifact/source rejected",
            ..
        }
    ));
    assert_eq!(calls, 1);
    drop(socket);
}

#[test]
fn retry_resets_partial_byte_and_file_counts_before_acceptance() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("a"), "aa").unwrap();
    fs::write(root.path().join("b"), "bb").unwrap();
    let mut calls = 0;
    let paths = scanner(root.path(), 4, 2, false, |path| {
        calls += 1;
        if calls == 2 {
            fs::remove_file(path).unwrap();
            fs::write(root.path().join(".replacement"), "cc").unwrap();
        }
    })
    .live()
    .unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&root.path().join(".replacement")));
    assert_eq!(
        paths
            .iter()
            .map(|p| fs::metadata(p).unwrap().len())
            .sum::<u64>(),
        4
    );
}

#[test]
fn replacing_the_root_directory_invalidates_live_and_strict_scans() {
    for live in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("scan");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("temporary"), "x").unwrap();
        let mut scan = scanner(&directory, 10, 10, false, |_| {
            fs::rename(&directory, root.path().join("previous")).unwrap();
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("temporary"), "x").unwrap();
        });
        let error = if live { scan.live() } else { scan.once() }.unwrap_err();
        assert!(matches!(
            error,
            ScanError::Rejected {
                reason: "scan root changed during traversal",
                ..
            }
        ));
    }
}

#[test]
fn observed_budget_excess_is_fatal_without_restart() {
    for (max_bytes, max_files) in [(3, 10), (10, 0)] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".over-cap"), "data").unwrap();
        let mut calls = 0;
        let error = scanner(root.path(), max_bytes, max_files, false, |_| {
            calls += 1;
        })
        .live()
        .unwrap_err();
        assert!(matches!(
            error,
            ScanError::Rejected {
                reason: "artifact/source storage cap exceeded",
                ..
            }
        ));
        assert_eq!(calls, 1);
    }
}
