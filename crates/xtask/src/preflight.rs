//! Runs the preflight checks of a CI profile once per gated commit.
//!
//! `ci-managed`, `ci-gpu-static` and `ci-cpu` each start with the same checks (policy,
//! source-size, formatting) on the same tree. The hub gate runs them one after another in one
//! checkout of the candidate commit, so the first profile that passes them records a marker
//! file, `target/validation/preflight-<commit>`, and a later profile skips them when that
//! marker exists for the same commit and the checkout is still clean. The marker is only
//! trusted for the exact commit in `HN_COMMIT` (the variable the hub runner sets), and only
//! while `HEAD` is that commit and `git status` reports no change. Without all three, the
//! checks run in full, as they do for a local run and in the GitHub workflows.

use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER_DIR: &str = "target/validation";
const MARKER_PREFIX: &str = "preflight-";

/// Runs `steps` unless this clean checkout of the commit in `HN_COMMIT` already passed them.
pub fn run(root: &Path, steps: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    run_for(root, std::env::var("HN_COMMIT").ok().as_deref(), steps)
}

fn run_for(
    root: &Path,
    commit: Option<&str>,
    steps: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let Some(commit) = commit.map(str::trim).filter(|c| is_commit_id(c)) else {
        return steps();
    };
    let commit = commit.to_ascii_lowercase();
    let clean = is_clean_checkout_of(root, &commit);
    let marker = marker_path(root, &commit);
    if marker.is_file() {
        if clean {
            eprintln!("xtask: preflight: already passed for {commit} on this clean tree; skipped");
            return Ok(());
        }
        eprintln!(
            "xtask: preflight: {commit} has passed, but this checkout is not a clean {commit}; running it"
        );
    }
    steps()?;
    // The checks only read the tree, but a marker for a tree that changed meanwhile would
    // vouch for something the checks never saw.
    if clean && is_clean_checkout_of(root, &commit) {
        record(root, &marker);
    }
    Ok(())
}

/// A full object name; anything else must not become part of a file name.
fn is_commit_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn marker_path(root: &Path, commit: &str) -> PathBuf {
    root.join(MARKER_DIR)
        .join(format!("{MARKER_PREFIX}{commit}"))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not start git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} exited with {}",
            args.join(" "),
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether `HEAD` is `commit` and `git status` shows no modified, staged or untracked file.
/// Any failure to find out counts as not clean, so the checks run.
fn is_clean_checkout_of(root: &Path, commit: &str) -> bool {
    let verdict = git(root, &["rev-parse", "HEAD"]).and_then(|head| {
        if head.trim().eq_ignore_ascii_case(commit) {
            git(root, &["status", "--porcelain", "--untracked-files=normal"])
                .map(|status| status.trim().is_empty())
        } else {
            Ok(false)
        }
    });
    verdict.unwrap_or_else(|error| {
        eprintln!("xtask: preflight: cannot confirm a clean checkout of {commit}: {error}");
        false
    })
}

/// Writes the marker and removes the markers of other commits. A failure only costs the
/// skip in a later profile, so it is reported and not returned.
fn record(root: &Path, marker: &Path) {
    let result = std::fs::create_dir_all(root.join(MARKER_DIR))
        .and_then(|()| std::fs::write(marker, b""))
        .and_then(|()| remove_other_markers(&root.join(MARKER_DIR), marker));
    if let Err(error) = result {
        eprintln!(
            "xtask: preflight: cannot record {}: {error}",
            marker.display()
        );
    }
}

fn remove_other_markers(dir: &Path, keep: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_marker = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(MARKER_PREFIX));
        if is_marker && path != keep && path.is_file() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;
    use crate::scratch::Directory;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .expect("git must be installed to run the preflight tests");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// A checkout with one commit, tracked `source.rs` and an ignored `target/`, like the hub
    /// gate's persistent checkout.
    fn checkout() -> (Directory, String) {
        let dir = Directory::new("preflight").unwrap();
        git(dir.path(), &["init", "-q"]);
        dir.write(".gitignore", "/target/\n").unwrap();
        dir.write("source.rs", "fn main() {}\n").unwrap();
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "first"]);
        let head = git(dir.path(), &["rev-parse", "HEAD"]);
        (dir, head)
    }

    fn commit_change(dir: &Directory) -> String {
        dir.write("source.rs", "fn main() { let _x = 1; }\n")
            .unwrap();
        git(dir.path(), &["commit", "-q", "-a", "-m", "second"]);
        git(dir.path(), &["rev-parse", "HEAD"])
    }

    fn marker(dir: &Directory, commit: &str) -> PathBuf {
        dir.path()
            .join("target/validation")
            .join(format!("preflight-{commit}"))
    }

    /// Steps that count how often they ran and then pass.
    fn passing(runs: &Cell<u32>) -> impl FnOnce() -> Result<(), String> + '_ {
        move || {
            runs.set(runs.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn a_later_profile_skips_the_checks_a_clean_commit_already_passed() {
        let (dir, head) = checkout();
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 1);
        assert!(marker(&dir, &head).is_file());
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 1, "the marker must stand for the whole gate");
    }

    #[test]
    fn without_a_commit_every_profile_runs_the_checks_and_leaves_no_marker() {
        let (dir, _head) = checkout();
        let runs = Cell::new(0);
        for _ in 0..3 {
            run_for(dir.path(), None, passing(&runs)).unwrap();
        }
        assert_eq!(runs.get(), 3);
        assert!(!dir.path().join("target/validation").exists());
    }

    #[test]
    fn a_dirty_tree_runs_the_checks_even_when_the_commit_has_a_marker() {
        let (dir, head) = checkout();
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        dir.write("source.rs", "fn main() { broken }\n").unwrap();
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 2, "a modified tracked file");
        git(dir.path(), &["checkout", "-q", "--", "source.rs"]);
        dir.write("extra.rs", "fn extra() {}\n").unwrap();
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 3, "an untracked file");
        std::fs::remove_file(dir.path().join("extra.rs")).unwrap();
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 3, "the clean tree trusts the marker again");
    }

    #[test]
    fn a_pass_on_a_dirty_tree_records_no_marker() {
        let (dir, head) = checkout();
        dir.write("extra.rs", "fn extra() {}\n").unwrap();
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert!(!marker(&dir, &head).exists());
        std::fs::remove_file(dir.path().join("extra.rs")).unwrap();
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 2, "the earlier pass covered another tree");
    }

    #[test]
    fn a_marker_for_another_commit_does_not_skip_the_checks() {
        let (dir, first) = checkout();
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&first), passing(&runs)).unwrap();
        let second = commit_change(&dir);
        run_for(dir.path(), Some(&second), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 2);
        assert!(marker(&dir, &second).is_file());
    }

    #[test]
    fn a_commit_that_is_not_checked_out_runs_the_checks_and_records_nothing() {
        let (dir, first) = checkout();
        let second = commit_change(&dir);
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&first), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 1);
        assert!(!marker(&dir, &first).exists());
        // A marker planted for the requested commit is not trusted when HEAD is elsewhere.
        std::fs::create_dir_all(dir.path().join("target/validation")).unwrap();
        std::fs::write(marker(&dir, &first), "").unwrap();
        run_for(dir.path(), Some(&first), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 2);
        assert_ne!(first, second);
    }

    #[test]
    fn a_failing_check_returns_its_error_and_records_no_marker() {
        let (dir, head) = checkout();
        let failure = run_for(dir.path(), Some(&head), || Err("policy failed".to_owned()));
        assert_eq!(failure, Err("policy failed".to_owned()));
        assert!(!marker(&dir, &head).exists());
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&head), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn a_value_that_is_not_a_commit_id_never_reads_or_writes_a_marker() {
        let (dir, _head) = checkout();
        let runs = Cell::new(0);
        for value in [
            "",
            "  ",
            "abc123",
            "../../escape",
            "HEAD",
            "zz".repeat(20).as_str(),
        ] {
            run_for(dir.path(), Some(value), passing(&runs)).unwrap();
        }
        assert_eq!(runs.get(), 6);
        assert!(!dir.path().join("target/validation").exists());
        assert!(!dir.path().join("escape").exists());
    }

    #[test]
    fn a_directory_that_is_not_a_git_checkout_runs_the_checks() {
        let dir = Directory::new("preflight-nogit").unwrap();
        let commit = "a".repeat(40);
        let runs = Cell::new(0);
        run_for(dir.path(), Some(&commit), passing(&runs)).unwrap();
        run_for(dir.path(), Some(&commit), passing(&runs)).unwrap();
        assert_eq!(runs.get(), 2);
        assert!(!marker(&dir, &commit).exists());
    }

    #[test]
    fn a_pass_removes_the_markers_of_other_commits_and_nothing_else() {
        let (dir, head) = checkout();
        let old = "b".repeat(40);
        dir.write(&format!("target/validation/preflight-{old}"), "")
            .unwrap();
        dir.write("target/validation/keep.txt", "build output")
            .unwrap();
        run_for(dir.path(), Some(&head), || Ok(())).unwrap();
        assert!(marker(&dir, &head).is_file());
        assert!(!marker(&dir, &old).exists());
        assert!(dir.path().join("target/validation/keep.txt").is_file());
    }
}
