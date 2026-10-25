use crate::{Error, cache};
use std::fs;

fn repository() -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    cache::git(root.path(), &["init", "--quiet"]).unwrap();
    commit(root.path());
    let head = cache::git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    (root, head.trim().into())
}

fn commit(root: &std::path::Path) {
    cache::git(
        root,
        &[
            "-c",
            "user.name=Original Games Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    )
    .unwrap();
}

#[test]
fn existing_clean_checkout_is_verified_without_rewriting_git_state() {
    let (root, head) = repository();
    let path = root.path().join(".git/HEAD");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let bytes = fs::read(&path).unwrap();
    cache::verify(root.path(), &head).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}

#[test]
fn wrong_revision_is_rejected_without_repairing_the_checkout() {
    let (root, head) = repository();
    let expected = "0".repeat(40);
    let error = cache::verify(root.path(), &expected).unwrap_err();
    assert!(matches!(error, Error::Revision { actual, .. } if actual == head));
    assert_eq!(
        cache::git(root.path(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim(),
        head
    );
}

#[test]
fn modified_and_untracked_content_are_rejected_without_removal() {
    let (root, head) = repository();
    let path = root.path().join("original.txt");
    fs::write(&path, "original").unwrap();
    assert!(matches!(
        cache::verify(root.path(), &head),
        Err(Error::Modified(_))
    ));
    cache::git(root.path(), &["add", "original.txt"]).unwrap();
    assert!(matches!(
        cache::verify(root.path(), &head),
        Err(Error::Modified(_))
    ));
    commit(root.path());
    let head = cache::git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    fs::write(&path, "changed").unwrap();
    assert!(matches!(
        cache::verify(root.path(), head.trim()),
        Err(Error::Modified(_))
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), "changed");
}

#[test]
fn git_failures_retain_the_operation_and_directory() {
    let root = tempfile::tempdir().unwrap();
    let error = cache::git(root.path(), &["rev-parse", "HEAD"]).unwrap_err();
    assert!(matches!(error, Error::Git { operation, path, message }
        if operation == "rev-parse HEAD" && path == root.path() && !message.is_empty()));
}
